use std::path::PathBuf;

use clap::Parser;
use nlzss11::{compress, decompress, DecompressError};
use thiserror::Error;

#[derive(Parser, Debug)]
#[clap(
    about = "(de)compress nlzss11 data (usually has .LZ as extension, also brresC also uses it)"
)]
enum Args {
    #[clap(about = "Compress a file")]
    Compress {
        #[clap(help = "Name of the file to compress")]
        filename: String,
        #[clap(help = "Name of the compressed file (default: filename + .LZ)")]
        out_filename: Option<String>,
    },
    #[clap(about = "Alias for compress")]
    C {
        #[clap(help = "Name of the file to compress")]
        filename: String,
        #[clap(help = "Name of the compressed file (default: filename + .LZ)")]
        out_filename: Option<String>,
    },
    #[clap(about = "Decompress a file")]
    Decompress {
        #[clap(help = "Name of the file to decompress")]
        filename: String,
        #[clap(help = "Name of the decompressed file (default: LZ gets stripped or .decompressed appended)")]
        out_filename: Option<String>,
    },
    #[clap(about = "Alias for decompress")]
    D {
        #[clap(help = "Name of the file to decompress")]
        filename: String,
        #[clap(help = "Name of the decompressed file (default: LZ gets stripped or .decompressed appended)")]
        out_filename: Option<String>,
    },
}

#[derive(Error, Debug)]
enum MyError {
    #[error("could not read {filename}: {os_error}")]
    FileRead {
        filename: String,
        os_error: std::io::Error,
    },
    #[error("could not write {filename}: {os_error}")]
    FileWrite {
        filename: String,
        os_error: std::io::Error,
    },
    #[error("error decompressing, file is probably invalid: {0:?}!")]
    DecompressError(DecompressError),
}

fn main() -> Result<(), MyError> {
    let args = Args::parse();
    match args {
        Args::Compress {
            filename,
            out_filename,
        }
        | Args::C {
            filename,
            out_filename,
        } => {
            let path = PathBuf::from(filename.clone());
            let out_filename = out_filename.unwrap_or_else(|| filename.clone() + ".LZ");
            let out_path = PathBuf::from(out_filename.clone());
            let uncompressed = std::fs::read(path).map_err(|e| MyError::FileRead {
                filename,
                os_error: e,
            })?;
            let compressed = compress(&uncompressed);
            std::fs::write(out_path, compressed).map_err(|e| MyError::FileWrite {
                filename: out_filename,
                os_error: e,
            })?;
        }
        Args::Decompress {
            filename,
            out_filename,
        }
        | Args::D {
            filename,
            out_filename,
        } => {
            let path = PathBuf::from(filename.clone());
            let out_filename = out_filename.unwrap_or_else(|| {
                if filename.ends_with(".LZ") {
                    filename[..filename.len() - 3].to_string()
                } else {
                    filename.clone() + ".decompressed"
                }
            });
            let out_path = PathBuf::from(out_filename.clone());
            let compressed = std::fs::read(path).map_err(|e| MyError::FileRead {
                filename,
                os_error: e,
            })?;
            let decompressed = decompress(&compressed).map_err(|e| MyError::DecompressError(e))?;
            std::fs::write(out_path, decompressed).map_err(|e| MyError::FileWrite {
                filename: out_filename,
                os_error: e,
            })?;
        }
    }
    Ok(())
}
