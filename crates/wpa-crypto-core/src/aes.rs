// SPDX-License-Identifier: ISC
//! AES-128 (FIPS 197), encrypt and decrypt of single 16-byte blocks. The S-boxes are generated at
//! compile time from the field definition, so no 512-byte table is transcribed by hand.

const fn rotl8(x: u8, s: u32) -> u8 {
    (x << s) | (x >> (8 - s))
}

/// FIPS 197 5.1.1: the multiplicative-inverse-then-affine S-box, built by walking the generator 3.
const fn build_sboxes() -> ([u8; 256], [u8; 256]) {
    let mut sbox = [0u8; 256];
    let mut inv = [0u8; 256];
    let mut p: u8 = 1;
    let mut q: u8 = 1;
    loop {
        // p *= 3 in GF(2^8)
        p = p ^ (p << 1) ^ if p & 0x80 != 0 { 0x1b } else { 0 };
        // q /= 3 in GF(2^8)
        q ^= q << 1;
        q ^= q << 2;
        q ^= q << 4;
        if q & 0x80 != 0 {
            q ^= 0x09;
        }
        let x = q ^ rotl8(q, 1) ^ rotl8(q, 2) ^ rotl8(q, 3) ^ rotl8(q, 4) ^ 0x63;
        sbox[p as usize] = x;
        inv[x as usize] = p;
        if p == 1 {
            break;
        }
    }
    sbox[0] = 0x63;
    inv[0x63] = 0;
    (sbox, inv)
}

const BOXES: ([u8; 256], [u8; 256]) = build_sboxes();
const SBOX: [u8; 256] = BOXES.0;
const ISBOX: [u8; 256] = BOXES.1;

#[inline]
fn xtime(x: u8) -> u8 {
    (x << 1) ^ if x & 0x80 != 0 { 0x1b } else { 0 }
}

fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut r = 0;
    while b != 0 {
        if b & 1 != 0 {
            r ^= a;
        }
        a = xtime(a);
        b >>= 1;
    }
    r
}

/// An expanded AES-128 key.
#[derive(Clone)]
pub struct Aes128 {
    rk: [[u8; 16]; 11],
}

impl Aes128 {
    pub fn new(key: &[u8; 16]) -> Aes128 {
        let mut rk = [[0u8; 16]; 11];
        rk[0] = *key;
        let mut rcon = 1u8;
        for r in 1..11 {
            let prev = rk[r - 1];
            let mut t = [SBOX[prev[13] as usize] ^ rcon, SBOX[prev[14] as usize], SBOX[prev[15] as usize], SBOX[prev[12] as usize]];
            rcon = xtime(rcon);
            for w in 0..4 {
                for i in 0..4 {
                    t[i] ^= prev[w * 4 + i];
                    rk[r][w * 4 + i] = t[i];
                }
            }
        }
        Aes128 { rk }
    }

    pub fn encrypt_block(&self, block: &[u8; 16]) -> [u8; 16] {
        let mut s = *block;
        xor(&mut s, &self.rk[0]);
        for r in 1..11 {
            for b in s.iter_mut() {
                *b = SBOX[*b as usize];
            }
            shift_rows(&mut s);
            if r != 10 {
                mix_columns(&mut s);
            }
            xor(&mut s, &self.rk[r]);
        }
        s
    }

    pub fn decrypt_block(&self, block: &[u8; 16]) -> [u8; 16] {
        let mut s = *block;
        xor(&mut s, &self.rk[10]);
        for r in (0..10).rev() {
            inv_shift_rows(&mut s);
            for b in s.iter_mut() {
                *b = ISBOX[*b as usize];
            }
            xor(&mut s, &self.rk[r]);
            if r != 0 {
                inv_mix_columns(&mut s);
            }
        }
        s
    }
}

fn xor(a: &mut [u8; 16], b: &[u8; 16]) {
    for i in 0..16 {
        a[i] ^= b[i];
    }
}

// State is column-major: byte i is row i%4, column i/4.
fn shift_rows(s: &mut [u8; 16]) {
    let t = *s;
    for c in 0..4 {
        for r in 0..4 {
            s[c * 4 + r] = t[((c + r) % 4) * 4 + r];
        }
    }
}

fn inv_shift_rows(s: &mut [u8; 16]) {
    let t = *s;
    for c in 0..4 {
        for r in 0..4 {
            s[((c + r) % 4) * 4 + r] = t[c * 4 + r];
        }
    }
}

fn mix_columns(s: &mut [u8; 16]) {
    for c in 0..4 {
        let a = [s[c * 4], s[c * 4 + 1], s[c * 4 + 2], s[c * 4 + 3]];
        s[c * 4] = xtime(a[0]) ^ (xtime(a[1]) ^ a[1]) ^ a[2] ^ a[3];
        s[c * 4 + 1] = a[0] ^ xtime(a[1]) ^ (xtime(a[2]) ^ a[2]) ^ a[3];
        s[c * 4 + 2] = a[0] ^ a[1] ^ xtime(a[2]) ^ (xtime(a[3]) ^ a[3]);
        s[c * 4 + 3] = (xtime(a[0]) ^ a[0]) ^ a[1] ^ a[2] ^ xtime(a[3]);
    }
}

fn inv_mix_columns(s: &mut [u8; 16]) {
    for c in 0..4 {
        let a = [s[c * 4], s[c * 4 + 1], s[c * 4 + 2], s[c * 4 + 3]];
        s[c * 4] = gmul(a[0], 14) ^ gmul(a[1], 11) ^ gmul(a[2], 13) ^ gmul(a[3], 9);
        s[c * 4 + 1] = gmul(a[0], 9) ^ gmul(a[1], 14) ^ gmul(a[2], 11) ^ gmul(a[3], 13);
        s[c * 4 + 2] = gmul(a[0], 13) ^ gmul(a[1], 9) ^ gmul(a[2], 14) ^ gmul(a[3], 11);
        s[c * 4 + 3] = gmul(a[0], 11) ^ gmul(a[1], 13) ^ gmul(a[2], 9) ^ gmul(a[3], 14);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sboxes_match_fips197_corners() {
        // FIPS 197 figure 7 / figure 14, written out by hand.
        assert_eq!(SBOX[0x00], 0x63);
        assert_eq!(SBOX[0x01], 0x7c);
        assert_eq!(SBOX[0x53], 0xed);
        assert_eq!(SBOX[0xff], 0x16);
        assert_eq!(ISBOX[0x00], 0x52);
        assert_eq!(ISBOX[0x63], 0x00);
        assert_eq!(ISBOX[0xff], 0x7d);
        for i in 0..256 {
            assert_eq!(ISBOX[SBOX[i] as usize] as usize, i);
        }
    }
}
