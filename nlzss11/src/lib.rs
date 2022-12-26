use byteorder::{ByteOrder, LE};

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum DecompressError {
    #[error("invalid magic")]
    InvalidMagic,
    #[error("invalid index: {0}")]
    InvalidIndex(usize),
    // TODO make better
    #[error("other error: {0}")]
    LibraryError(&'static str),
}

struct LzssCode {
    distance: u32,
    length: u32,
}

impl LzssCode {
    fn read(buf: &[u8]) -> Option<(LzssCode, usize)> {
        let pair = u16::from_be_bytes(buf[..2].try_into().ok()?) as u32;
        Some(match pair & 0xF000 {
            0 => {
                // 0000LLLL LLLLDDDD DDDDDDDD
                // L + 0x11, D + 1
                // 255 + 17 >= length >= 17
                let length = (pair >> 4) + 0x11;
                let distance = (((pair & 0xF) << 8) as u32 | *buf.get(2)? as u32) + 1;
                (LzssCode { distance, length }, 3)
            }
            0x1000 => {
                // 0001LLLL LLLLLLLL LLLLDDDD DDDDDDDD
                // L + 0x111, D + 1
                // 2^16 + 255 + 17 >= length >= 256 + 17
                let ext_pair = u16::from_be_bytes(buf[2..4].try_into().ok()?) as u32;
                let length = ((pair & 0xFFF) << 4 | ext_pair >> 12) + 0x111;
                let distance = (ext_pair & 0xFFF) + 1;
                (LzssCode { distance, length }, 4)
            }
            _ => {
                // LLLLDDDD DDDDDDDD
                // L + 1, D + 1
                // 15 + 1 >= length >= 3
                let length = (pair >> 12) + 1;
                let distance = (pair & 0xFFF) + 1;
                (LzssCode { distance, length }, 2)
            }
        })
    }

    pub fn write(&self, out_buf: &mut Vec<u8>) {
        let adj_dist = self.distance - 1;
        if self.length >= 0x111 {
            let adj_len = self.length - 0x111;
            out_buf.push(((1 << 4) + (adj_len >> 12)) as u8);
            out_buf.push((adj_len >> 4) as u8);
            out_buf.push((((adj_len & 0xF) << 4) + (adj_dist >> 8)) as u8);
            out_buf.push((adj_dist & 0xFF) as u8);
        } else if self.length >= 0x11 {
            let adj_len = self.length - 0x11;
            out_buf.push((adj_len >> 4) as u8);
            out_buf.push((((adj_len & 0xF) << 4) + (adj_dist >> 8)) as u8);
            out_buf.push((adj_dist & 0xFF) as u8);
        } else {
            let adj_len = self.length - 1;
            out_buf.push(((adj_len << 4) + (adj_dist >> 8)) as u8);
            out_buf.push((adj_dist & 0xFF) as u8);
        }
    }
}

#[inline(always)]
fn get_or_oob_err(data: &[u8], pos: usize) -> Result<u8, DecompressError> {
    data.get(pos)
        .copied()
        .ok_or(DecompressError::InvalidIndex(pos))
}

pub fn decompress(data: &[u8]) -> Result<Vec<u8>, DecompressError> {
    if data.len() < 4 {
        return Err(DecompressError::LibraryError("Too short"));
    }
    if data[0] != 0x11 {
        return Err(DecompressError::InvalidMagic);
    }
    let mut pos = 4;
    let mut out_size: usize = LE::read_u24(&data[1..]) as usize;
    if out_size == 0 {
        if data.len() < 8 {
            return Err(DecompressError::LibraryError("Too short"));
        }
        out_size = LE::read_u32(&data[4..]) as usize;
    }
    let mut out_buf = Vec::with_capacity(out_size);

    let mut group_header = 0;
    let mut remaining_chunks = 0;
    while out_buf.len() < out_buf.capacity() {
        // one byte indicates if the next 8 blocks are literals or backreferences
        if remaining_chunks == 0 {
            group_header = get_or_oob_err(data, pos)?;
            pos += 1;
            remaining_chunks = 8;
        }
        if (group_header & 0x80) == 0 {
            out_buf.push(get_or_oob_err(data, pos)?);
            pos += 1;
        } else {
            let (LzssCode { distance, length }, advance) =
                LzssCode::read(&data[pos..]).ok_or(DecompressError::InvalidIndex(data.len()))?;

            pos += advance;

            let cpy_start = out_buf
                .len()
                .checked_sub(distance as usize)
                .ok_or(DecompressError::InvalidIndex(0))?;
            if distance > length {
                // region to copy doesn't overlap the region it's copied to
                out_buf.extend_from_within(cpy_start..cpy_start + length as usize);
            } else {
                for cpy_pos in cpy_start..cpy_start + length as usize {
                    // it shouldn't be possible to end up in the default of unwrap_or
                    out_buf.push(out_buf.get(cpy_pos).copied().unwrap_or(0));
                }
            }
        }

        group_header <<= 1;
        remaining_chunks -= 1;
    }
    Ok(out_buf)
}

// https://github.com/PSeitz/lz4_flex/blob/c17d3b110325211f9e63c897add5fad09ddd8ef1/src/block/hashtable.rs#L16
#[inline]
fn make_hash(sequence: [u8; 4]) -> u32 {
    (u32::from_ne_bytes(sequence).wrapping_mul(2654435761_u32)) >> 16
}

const HASH_COUNT: usize = 4096 * 16; // has to be power of 2

struct MatchSearcher {
    search_dict: [u32; HASH_COUNT],
}

impl MatchSearcher {
    pub fn new() -> Self {
        MatchSearcher {
            search_dict: [u32::MAX; HASH_COUNT],
        }
    }
    pub fn submit_val(&mut self, data: &[u8], cur_pos: u32) {
        let rest = &data[cur_pos as usize..];
        if rest.len() < 4 {
            return;
        }
        let hash = make_hash(rest[..4].try_into().unwrap()) % HASH_COUNT as u32;
        self.search_dict[hash as usize] = cur_pos;
    }

    pub fn get_lz_code(&self, data: &[u8], cur_pos: u32) -> Option<(u32, u32)> {
        let rest = &data[cur_pos as usize..];
        if rest.len() < 4 {
            return None;
        }
        let hash = make_hash(rest[..4].try_into().unwrap()) % HASH_COUNT as u32;
        let prev = self.search_dict[hash as usize];
        if prev == u32::MAX {
            return None;
        }
        let match_backref = cur_pos.wrapping_sub(prev);
        if match_backref > TOTAL_BACKREF_POS {
            return None;
        }
        let match_len = data[cur_pos as usize..]
            .iter()
            .zip(data[prev as usize..].iter())
            .take_while(|&(a, b)| a == b)
            .count();
        if match_len < 4 {
            return None;
        }
        Some((match_backref, (match_len as u32).min(TOTAL_BACKREF_LEN)))
        // None
    }
}

const TOTAL_BACKREF_LEN: u32 = 0x10110;
const TOTAL_BACKREF_POS: u32 = 0xFFF;

pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut searcher = MatchSearcher::new();

    let mut out_buf: Vec<u8> = Vec::with_capacity(data.len());
    // write magic
    out_buf.push(0x11);
    // very big archives
    // little endian data length
    if data.len() < 0xFFFFFF {
        let mut len_buf = [0; 3];
        LE::write_u24(&mut len_buf, data.len() as u32);
        out_buf.extend_from_slice(&len_buf);
    } else if data.len() < 0xFFFFFFFF {
        out_buf.extend([0, 0, 0]);
        out_buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    }

    let mut group_header_pos = out_buf.len();
    out_buf.push(0);
    let mut group_header = 0;
    let mut group_header_count = 0;

    // go through the input in 3 byte chunks
    let mut pos: usize = 0;

    while pos < data.len() {
        if group_header_count == 8 {
            out_buf[group_header_pos] = group_header;
            group_header_pos = out_buf.len();
            out_buf.push(0);
            group_header = 0;
            group_header_count = 0;
        }
        if let Some((backref_dist, backref_len)) = searcher.get_lz_code(data, pos as u32) {
            group_header <<= 1;
            group_header += 1;
            group_header_count += 1;
            LzssCode {
                length: backref_len,
                distance: backref_dist,
            }
            .write(&mut out_buf);
            for p in pos..(pos + backref_len as usize) {
                searcher.submit_val(data, p as u32);
            }
            pos += backref_len as usize;
            // TODO: submit vals?
        } else {
            group_header <<= 1;
            group_header_count += 1;
            out_buf.push(data[pos]);
            searcher.submit_val(data, pos as u32);
            pos += 1;
        }
    }
    if group_header_count != 0 {
        group_header <<= 8 - group_header_count;
        out_buf[group_header_pos] = group_header;
    }
    out_buf
}

#[cfg(test)]
mod test {
    use super::LzssCode;

    #[test]
    pub fn test_roundtrip() {
        let mut buf = Vec::new();
        for len in 3..0x1011 {
            for dist in 1..0xFFF {
                buf.clear();
                LzssCode {
                    distance: dist,
                    length: len,
                }
                .write(&mut buf);
                let (LzssCode { distance, length }, read_bytes) = LzssCode::read(&buf)
                    .ok_or_else(|| format!("dist: {}, len: {}", dist, len))
                    .unwrap();
                // assert_eq!(len, length);
                // assert_eq!(dist, distance);
                // assert_eq!(read_bytes, buf.len());
                if !(len == length && dist == distance && read_bytes == buf.len()) {
                    panic!("err len: {}, dist: {}", len, dist);
                }
            }
        }
    }
}
