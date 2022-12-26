# nlzss11

Library for (de)compressing data according to the algorithm found in games like The Legend of Zelda: Skyward Sword

For this, there are 2 functions:

```rust
fn compress(data: &[u8]) -> Vec<u8>;
fn decompress(data: &[u8]) -> Result<Vec<u8>, DecompressError>;
```
