//! Minimal Adler-32 implementation matching zlib's `adler32(1, data, len)`,
//! which is what `Marshal.cpp` uses for its optional stream checksum.

const MOD_ADLER: u32 = 65521;

pub fn adler32(seed: u32, data: &[u8]) -> u32 {
    let mut a = seed & 0xffff;
    let mut b = (seed >> 16) & 0xffff;

    // Process in chunks to delay the modulo, same trick zlib uses, while
    // staying well clear of u32 overflow (5552 is the standard NMAX bound).
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= MOD_ADLER;
        b %= MOD_ADLER;
    }

    (b << 16) | a
}
