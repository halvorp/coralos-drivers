// SPDX-License-Identifier: ISC
//! AES Key Wrap (RFC 3394) with a 128-bit KEK, in place. The EAPOL-Key Key Data field of
//! descriptor versions 2 and 3 is wrapped with it (802.11-2020 12.7.2 h/i).

use crate::aes::Aes128;

/// The RFC 3394 default initial value.
pub const DEFAULT_IV: [u8; 8] = [0xa6; 8]; // RFC 3394 2.2.3.1

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapRefusal {
    /// Plaintext/ciphertext length not a multiple of 8, or fewer than two 64-bit blocks of
    /// plaintext (RFC 3394 2: n >= 2). Carries the length given.
    Length(usize),
    /// The integrity register after unwrapping was not the default IV: wrong KEK or a damaged frame.
    Integrity,
}

/// Unwrap `buf` (`n + 1` blocks of 8 bytes) in place; on success the plaintext is `buf[8..]`.
pub fn unwrap_in_place(kek: &[u8; 16], buf: &mut [u8]) -> Result<(), WrapRefusal> {
    if buf.len() % 8 != 0 || buf.len() < 24 {
        return Err(WrapRefusal::Length(buf.len()));
    }
    let n = buf.len() / 8 - 1;
    let aes = Aes128::new(kek);
    let mut a = [0u8; 8];
    a.copy_from_slice(&buf[..8]);
    for j in (0..6).rev() {
        for i in (1..=n).rev() {
            let t = (n * j + i) as u64;
            let tb = t.to_be_bytes();
            let mut blk = [0u8; 16];
            for k in 0..8 {
                blk[k] = a[k] ^ tb[k];
            }
            blk[8..].copy_from_slice(&buf[i * 8..i * 8 + 8]);
            let out = aes.decrypt_block(&blk);
            a.copy_from_slice(&out[..8]);
            buf[i * 8..i * 8 + 8].copy_from_slice(&out[8..]);
        }
    }
    if !crate::ct_eq(&a, &DEFAULT_IV) {
        return Err(WrapRefusal::Integrity);
    }
    buf[..8].copy_from_slice(&a);
    Ok(())
}

/// Wrap the plaintext in `buf[8..]` in place, writing the integrity block to `buf[..8]`.
pub fn wrap_in_place(kek: &[u8; 16], buf: &mut [u8]) -> Result<(), WrapRefusal> {
    if buf.len() % 8 != 0 || buf.len() < 24 {
        return Err(WrapRefusal::Length(buf.len()));
    }
    let n = buf.len() / 8 - 1;
    let aes = Aes128::new(kek);
    let mut a = DEFAULT_IV;
    for j in 0..6 {
        for i in 1..=n {
            let mut blk = [0u8; 16];
            blk[..8].copy_from_slice(&a);
            blk[8..].copy_from_slice(&buf[i * 8..i * 8 + 8]);
            let out = aes.encrypt_block(&blk);
            let tb = ((n * j + i) as u64).to_be_bytes();
            for k in 0..8 {
                a[k] = out[k] ^ tb[k];
            }
            buf[i * 8..i * 8 + 8].copy_from_slice(&out[8..]);
        }
    }
    buf[..8].copy_from_slice(&a);
    Ok(())
}
