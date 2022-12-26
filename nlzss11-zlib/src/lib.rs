use std::ffi::{c_int, c_void};

extern "C" {
    fn zng_compress2(dest: *mut u8, dest_len: *mut usize, source: *const u8,
        source_len: usize, level: c_int, handle_match: unsafe extern "C" fn(*mut c_void, u32, u32),
        handle_match_userdata: *mut c_void
    ) -> c_int;
}

struct Lzss11Writer {
    result: Vec<u8>,
    group_header: u8,
    group_header_count: u8,
    group_header_offset: usize
}

impl Lzss11Writer {
    pub fn new(dest: &mut Vec<u8>, uncompressed_size: u32) -> Self {
        let mut buffer = Vec::new();
        std::mem::swap(dest, &mut buffer);
        // write magic
        buffer.push(0x11);
        // handle very big archives
        if uncompressed_size < 0xFF_FF_FF {
            buffer.extend_from_slice(&uncompressed_size.to_be_bytes()[..3]);
        } else {
            buffer.extend_from_slice(&[0,0,0]);
            buffer.extend_from_slice(&uncompressed_size.to_le_bytes());
        }
        let group_header_offset = buffer.len();
        // first group header
        buffer.push(0);
        Lzss11Writer { result: buffer, group_header: 0, group_header_count: 0, group_header_offset }
    }

    pub fn handle_match(&mut self, distance: u32, length: u32) {
        if self.group_header_count == 8 {
            self.result[self.group_header_offset] = self.group_header;
            self.group_header_offset = self.result.len();
            self.result.push(0);
            self.group_header = 0;
            self.group_header_count = 0;
        }
        if distance == 0 {
            self.group_header <<= 1;
            self.group_header_count += 1;
            // this is a literal
            self.result.push(length as u8);
        } else {
            let adj_dist = distance - 1;
            if length >= 0x111 {
                let adj_len = length - 0x111;
                self.result.push(((1 << 4) + (adj_len >> 12)) as u8);
                self.result.push((adj_len >> 4) as u8);
                self.result.push((((adj_len & 0xF) << 4) + (adj_dist >> 8)) as u8);
                self.result.push((adj_dist & 0xFF) as u8);
            } else if length >= 0x11 {
                let adj_len = length - 0x11;
                self.result.push((adj_len >> 4) as u8);
                self.result.push((((adj_len & 0xF) << 4) + (adj_dist >> 8)) as u8);
                self.result.push((adj_dist & 0xFF) as u8);
            } else {
                let adj_len = length - 1;
                self.result.push(((adj_len << 4) + (adj_dist >> 8)) as u8);
                self.result.push((adj_dist & 0xFF) as u8);
            }
            self.group_header <<= 1;
            self.group_header += 1;
            self.group_header_count += 1;
        }
    }

    fn finish(&mut self) {
        if self.group_header_count != 0 {
            self.group_header <<= 8 - self.group_header_count;
            self.result[self.group_header_offset] = self.group_header;
        }
    }
}

extern "C" fn handle_match(user_data: *mut c_void, distance: u32, length: u32) {
    let writer = unsafe { &mut *(user_data as *mut Lzss11Writer) };
    writer.handle_match(distance, length);
}

pub fn compress_with_zlib_into(data: &[u8], out_buf: &mut Vec<u8>, level: i32) {
    let mut writer = Lzss11Writer::new(out_buf, data.len() as u32);
    let mut dummy = [0u8; 8];
    let mut dummy_outsize = dummy.len();
    let result = unsafe { zng_compress2(dummy.as_mut_ptr(), &mut dummy_outsize as *mut usize, data.as_ptr(), data.len(), level, handle_match, (&mut writer) as *mut Lzss11Writer as *mut c_void) };
    if result != 0 {
        panic!("zng_compress failed");
    }
    writer.finish();
    std::mem::swap(&mut writer.result, out_buf);
}

#[cfg(test)]
mod tests {
}
