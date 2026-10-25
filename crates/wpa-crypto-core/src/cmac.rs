// SPDX-License-Identifier: ISC
//! AES-128-CMAC (RFC 4493), the EAPOL-Key MIC of key descriptor version 3.

use crate::aes::Aes128;

const RB: u8 = 0x87; // RFC 4493 2.3

fn dbl(b: &[u8; 16]) -> [u8; 16] {
    let mut o = [0u8; 16];
    let mut carry = 0;
    for i in (0..16).rev() {
        o[i] = (b[i] << 1) | carry;
        carry = b[i] >> 7;
    }
    if carry != 0 {
        o[15] ^= RB;
    }
    o
}

/// CMAC of the concatenation of `parts` under `key`.
pub fn aes_cmac(key: &[u8; 16], parts: &[&[u8]]) -> [u8; 16] {
    let aes = Aes128::new(key);
    let k1 = dbl(&aes.encrypt_block(&[0; 16]));
    let k2 = dbl(&k1);
    let total: usize = parts.iter().map(|p| p.len()).sum();
    let blocks = if total == 0 { 1 } else { (total + 15) / 16 };
    let complete = total != 0 && total % 16 == 0;

    let mut x = [0u8; 16];
    let mut cur = [0u8; 16];
    let mut fill = 0usize;
    let mut done = 0usize; // blocks already chained
    for p in parts {
        for &byte in p.iter() {
            if fill == 16 {
                // a full block that is not the last: chain it
                for i in 0..16 {
                    x[i] ^= cur[i];
                }
                x = aes.encrypt_block(&x);
                done += 1;
                fill = 0;
            }
            cur[fill] = byte;
            fill += 1;
        }
    }
    debug_assert_eq!(done + 1, blocks);
    let mut last = [0u8; 16];
    if complete {
        for i in 0..16 {
            last[i] = cur[i] ^ k1[i];
        }
    } else {
        last[..fill].copy_from_slice(&cur[..fill]);
        last[fill] = 0x80;
        for i in 0..16 {
            last[i] ^= k2[i];
        }
    }
    for i in 0..16 {
        x[i] ^= last[i];
    }
    aes.encrypt_block(&x)
}
