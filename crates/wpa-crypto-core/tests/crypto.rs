// SPDX-License-Identifier: ISC
//! Published vectors, written out by hand: FIPS 180-4 / 197, RFC 2202, 4231, 6070, 3394, 4493,
//! IEEE 802.11i-2004 H.3 (PRF) and 802.11-2020 J.4 (passphrase mapping). Vectors marked ORACLE
//! have no published value for that exact input; they were produced by OpenSSL-backed Python
//! (hashlib / hmac / cryptography) and pin this crate to an independent implementation.

use wpa_crypto_core::aes::Aes128;
use wpa_crypto_core::cmac::aes_cmac;
use wpa_crypto_core::hash::{hmac_sha1, hmac_sha256, sha1, sha256, Digest, Sha1, Sha256};
use wpa_crypto_core::kdf::{derive_ptk, kdf_sha256, pmkid, prf_sha1, KdfKind, PTK_LABEL};
use wpa_crypto_core::keywrap::{unwrap_in_place, wrap_in_place, WrapRefusal};
use wpa_crypto_core::mic;
use wpa_crypto_core::pbkdf2::{passphrase_to_pmk, pbkdf2_hmac_sha1, PskRefusal};
use wpa_crypto_core::{ct_eq, Pmk};

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

#[test]
fn sha1_fips180_vectors() {
    assert_eq!(sha1(b"abc"), hex("a9993e364706816aba3e25717850c26c9cd0d89d")[..]);
    assert_eq!(sha1(b""), hex("da39a3ee5e6b4b0d3255bfef95601890afd80709")[..]);
    assert_eq!(
        sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        hex("84983e441c3bd26ebaae4aa1f95129e5e54670f1")[..]
    );
}

#[test]
fn sha256_fips180_vectors() {
    assert_eq!(
        sha256(b"abc"),
        hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")[..]
    );
    assert_eq!(
        sha256(b""),
        hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")[..]
    );
    assert_eq!(
        sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        hex("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1")[..]
    );
}

/// Every message length 0..200 crosses the 55/56/63/64 padding boundaries. ORACLE.
#[test]
fn sha_padding_boundaries_every_length() {
    let mut acc1 = Vec::new();
    let mut acc2 = Vec::new();
    for i in 0..200usize {
        let m: Vec<u8> = (0..i).map(|j| ((j * 7 + i) & 0xff) as u8).collect();
        acc1.extend_from_slice(&sha1(&m));
        acc2.extend_from_slice(&sha256(&m));
    }
    assert_eq!(sha1(&acc1), hex("c2add6cb3908952ddaf192d7f14c30588649e3b5")[..]);
    assert_eq!(sha256(&acc2), hex("8c7c9ff69da76fc28823a1cb97268672663e628e1ea955868b16db46bab0545d")[..]);
}

#[test]
fn streaming_in_odd_chunks_equals_one_shot() {
    let m: Vec<u8> = (0..300u32).map(|x| (x * 31 % 251) as u8).collect();
    for step in [1usize, 3, 63, 64, 65, 129] {
        let mut a = Sha1::new();
        let mut b = Sha256::new();
        for c in m.chunks(step) {
            a.update(c);
            b.update(c);
        }
        let (mut oa, mut ob) = ([0u8; 20], [0u8; 32]);
        a.finish(&mut oa);
        b.finish(&mut ob);
        assert_eq!(oa, sha1(&m), "sha1 step {step}");
        assert_eq!(ob, sha256(&m), "sha256 step {step}");
    }
}

#[test]
fn hmac_sha1_rfc2202() {
    assert_eq!(
        hmac_sha1(&[0x0b; 20], &[b"Hi There"]),
        hex("b617318655057264e28bc0b6fb378c8ef146be00")[..]
    );
    assert_eq!(
        hmac_sha1(b"Jefe", &[b"what do ya ", b"want for nothing?"]),
        hex("effcdf6ae5eb2fa2d27416d5f184df9c259a7c79")[..]
    );
    assert_eq!(
        hmac_sha1(&[0xaa; 80], &[b"Test Using Larger Than Block-Size Key - Hash Key First"]),
        hex("aa4ae5e15272d00e95705637ce8a3b55ed402112")[..]
    );
}

#[test]
fn hmac_sha256_rfc4231() {
    assert_eq!(
        hmac_sha256(&[0x0b; 20], &[b"Hi There"]),
        hex("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")[..]
    );
    assert_eq!(
        hmac_sha256(b"Jefe", &[b"what do ya want for nothing?"]),
        hex("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")[..]
    );
    // RFC 4231 test case 6: key longer than the block.
    assert_eq!(
        hmac_sha256(&[0xaa; 131], &[b"Test Using Larger Than Block-Size Key - Hash Key First"]),
        hex("60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54")[..]
    );
}

#[test]
fn pbkdf2_rfc6070() {
    let mut o = [0u8; 20];
    pbkdf2_hmac_sha1(b"password", b"salt", 1, &mut o);
    assert_eq!(o, hex("0c60c80f961f0e71f3a9b524af6012062fe037a6")[..]);
    pbkdf2_hmac_sha1(b"password", b"salt", 2, &mut o);
    assert_eq!(o, hex("ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957")[..]);
    pbkdf2_hmac_sha1(b"password", b"salt", 4096, &mut o);
    assert_eq!(o, hex("4b007901b765489abead49d926f721d065a429c1")[..]);
    // A 25-byte output spans two HMAC blocks.
    let mut o25 = [0u8; 25];
    pbkdf2_hmac_sha1(
        b"passwordPASSWORDpassword",
        b"saltSALTsaltSALTsaltSALTsaltSALTsalt",
        4096,
        &mut o25,
    );
    assert_eq!(o25, hex("3d2eec4fe41c849b80c8d83662c0e44a8b291a964cf2f07038")[..]);
}

#[test]
fn passphrase_to_pmk_80211_j4_vectors() {
    // IEEE 802.11-2020 J.4.1 test case 1.
    let p = passphrase_to_pmk(b"password", b"IEEE").unwrap();
    assert_eq!(p.as_bytes()[..], hex("f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e")[..]);
    // J.4.1 test case 2.
    let p = passphrase_to_pmk(b"ThisIsAPassword", b"ThisIsASSID").unwrap();
    assert_eq!(p.as_bytes()[..], hex("0dc0d6eb90555ed6419756b9a15ec3e3209b63df707dd508d14581f8982721af")[..]);
    // J.4.1 test case 3 shape: the 63-character maximum. ORACLE.
    let long = [b'a'; 63];
    let p = passphrase_to_pmk(&long, b"ThisIsASSID").unwrap();
    assert_eq!(p.as_bytes()[..], hex("ba5df9b4a953fb99ffc9b62ae96a419ff45673a0ee11534ee90dfa1839cd6068")[..]);
}

#[test]
fn passphrase_refusals_are_named_and_never_carry_the_secret() {
    assert_eq!(passphrase_to_pmk(b"short", b"S").err(), Some(PskRefusal::PassphraseLength(5)));
    assert_eq!(passphrase_to_pmk(b"1234567", b"S").err(), Some(PskRefusal::PassphraseLength(7)));
    assert_eq!(passphrase_to_pmk(&[b'a'; 64], b"S").err(), Some(PskRefusal::PassphraseLength(64)));
    assert_eq!(passphrase_to_pmk(b"abcdefg\x7f", b"S").err(), Some(PskRefusal::PassphraseNotAscii(7)));
    assert_eq!(passphrase_to_pmk(b"abcdefgh", b"").err(), Some(PskRefusal::SsidLength(0)));
    assert_eq!(passphrase_to_pmk(b"abcdefgh", &[b's'; 33]).err(), Some(PskRefusal::SsidLength(33)));
    assert!(passphrase_to_pmk(b"abcdefgh", &[b's'; 32]).is_ok());
    assert!(passphrase_to_pmk(&[b'a'; 8], b"S").is_ok());
}

#[test]
fn prf_sha1_ieee80211i_h3_vectors() {
    let mut o = [0u8; 24];
    prf_sha1(&[0x0b; 20], b"prefix", b"Hi There", &mut o); // 802.11i-2004 H.3 case 1
    assert_eq!(o, hex("bcd4c650b30b9684951829e0d75f9d54b862175ed9f00606")[..]);
    prf_sha1(b"Jefe", b"prefix", b"what do ya want for nothing?", &mut o); // case 2
    assert_eq!(o, hex("51f4de5b33f249adf81aeb713a3c20f4fe631446fabdfa58")[..]);
    prf_sha1(&[0xaa; 80], b"prefix", &[0xdd; 50], &mut o); // case 3
    assert_eq!(o, hex("c7ec5354f38fe551fc570fe06d76021a9b24c71d54a5c9af")[..]);
}

const AA: [u8; 6] = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
const SPA: [u8; 6] = [0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb];

fn test_pmk() -> Pmk {
    passphrase_to_pmk(b"password", b"IEEE").unwrap()
}

/// ORACLE: PTK for PMK(password, IEEE), AA/SPA above, ANonce 0x01*32, SNonce 0x02*32.
#[test]
fn ptk_prf_sha1_and_split() {
    let p = derive_ptk(KdfKind::PrfSha1, &test_pmk(), &AA, &SPA, &[1; 32], &[2; 32]);
    let want = hex("ad05dd4d9564a84f8efd19b420ef4b37ca35df205c03668b90720e02b7fea16fed368b487ede42acc203d6a4e2d73bd5");
    assert_eq!(p.kck[..], want[0..16]);
    assert_eq!(p.kek[..], want[16..32]);
    assert_eq!(p.tk[..], want[32..48]);
}

#[test]
fn ptk_is_symmetric_in_address_and_nonce_order() {
    let a = derive_ptk(KdfKind::PrfSha1, &test_pmk(), &AA, &SPA, &[1; 32], &[2; 32]);
    let b = derive_ptk(KdfKind::PrfSha1, &test_pmk(), &SPA, &AA, &[2; 32], &[1; 32]);
    assert_eq!(a.tk, b.tk);
    assert_eq!(a.kck, b.kck);
}

/// ORACLE.
#[test]
fn ptk_kdf_sha256() {
    let p = derive_ptk(KdfKind::KdfSha256, &test_pmk(), &AA, &SPA, &[1; 32], &[2; 32]);
    let want = hex("513ccc1de0fe12e32db5bf999312f6981a816f76c772e202fa5c92b8818c7bb08675ee2d01a423b602b98f1499f622ac");
    assert_eq!(p.kck[..], want[0..16]);
    assert_eq!(p.kek[..], want[16..32]);
    assert_eq!(p.tk[..], want[32..48]);
    // The output length is mixed into every block: a 32-byte request is not a prefix of 48.
    let mut o = [0u8; 32];
    let mut buf = Vec::new();
    buf.extend_from_slice(&AA);
    buf.extend_from_slice(&SPA);
    buf.extend_from_slice(&[1; 32]);
    buf.extend_from_slice(&[2; 32]);
    kdf_sha256(test_pmk().as_bytes(), PTK_LABEL, &buf, &mut o);
    assert_eq!(o, hex("74205e47dd98e7d61afc7251220585c282cbdf0da6ba3d258baa1c81615576c6")[..]);
    assert_ne!(o[..], want[..32]);
}

#[test]
fn ptk_differs_between_kdf_kinds() {
    let a = derive_ptk(KdfKind::PrfSha1, &test_pmk(), &AA, &SPA, &[1; 32], &[2; 32]);
    let b = derive_ptk(KdfKind::KdfSha256, &test_pmk(), &AA, &SPA, &[1; 32], &[2; 32]);
    assert_ne!(a.kck, b.kck);
}

/// ORACLE.
#[test]
fn pmkid_both_hashes() {
    assert_eq!(pmkid(KdfKind::PrfSha1, &test_pmk(), &AA, &SPA)[..], hex("0a258b4cb64a0e8a4fda11493f670531")[..]);
    assert_eq!(pmkid(KdfKind::KdfSha256, &test_pmk(), &AA, &SPA)[..], hex("6e78b7d63b73e1f0a0d8937bf5e61712")[..]);
}

#[test]
fn aes128_fips197_vectors() {
    // FIPS 197 C.1
    let key: [u8; 16] = core::array::from_fn(|i| i as u8);
    let pt: [u8; 16] = core::array::from_fn(|i| (i as u8) * 0x11);
    let a = Aes128::new(&key);
    let ct = a.encrypt_block(&pt);
    assert_eq!(ct[..], hex("69c4e0d86a7b0430d8cdb78070b4c55a")[..]);
    assert_eq!(a.decrypt_block(&ct), pt);
    // FIPS 197 appendix B
    let key: [u8; 16] = hex("2b7e151628aed2a6abf7158809cf4f3c").try_into().unwrap();
    let pt: [u8; 16] = hex("3243f6a8885a308d313198a2e0370734").try_into().unwrap();
    let a = Aes128::new(&key);
    assert_eq!(a.encrypt_block(&pt)[..], hex("3925841d02dc09fbdc118597196a0b32")[..]);
    assert_eq!(a.decrypt_block(&a.encrypt_block(&pt)), pt);
}

#[test]
fn key_wrap_rfc3394_4_1() {
    let kek: [u8; 16] = core::array::from_fn(|i| i as u8);
    let mut buf = [0u8; 24];
    for (i, b) in hex("00112233445566778899aabbccddeeff").iter().enumerate() {
        buf[8 + i] = *b;
    }
    wrap_in_place(&kek, &mut buf).unwrap();
    assert_eq!(buf[..], hex("1fa68b0a8112b447aef34bd8fb5a7b829d3e862371d2cfe5")[..]);
    unwrap_in_place(&kek, &mut buf).unwrap();
    assert_eq!(buf[8..], hex("00112233445566778899aabbccddeeff")[..]);
}

/// ORACLE: a 32-byte key, as a GTK KDE padded to blocks.
#[test]
fn key_wrap_longer_payload_roundtrip() {
    let kek: [u8; 16] = core::array::from_fn(|i| i as u8);
    let mut buf = [0u8; 40];
    for i in 0..32 {
        buf[8 + i] = 100 + i as u8;
    }
    wrap_in_place(&kek, &mut buf).unwrap();
    assert_eq!(buf[..], hex("8376b021c4006411b9c12c40302db34f34f4e1963a8adaab59cac595f70246f439d8595f820099bf")[..]);
    unwrap_in_place(&kek, &mut buf).unwrap();
    assert_eq!(buf[8], 100);
}

#[test]
fn key_unwrap_refusals() {
    let kek = [7u8; 16];
    let mut bad = [0u8; 24];
    assert_eq!(unwrap_in_place(&kek, &mut bad), Err(WrapRefusal::Integrity));
    let mut short = [0u8; 16];
    assert_eq!(unwrap_in_place(&kek, &mut short), Err(WrapRefusal::Length(16)));
    let mut odd = [0u8; 27];
    assert_eq!(unwrap_in_place(&kek, &mut odd), Err(WrapRefusal::Length(27)));
    // A flipped ciphertext bit is caught by the integrity block.
    let mut buf = [0u8; 24];
    buf[8..].copy_from_slice(&[9u8; 16]);
    wrap_in_place(&kek, &mut buf).unwrap();
    buf[20] ^= 1;
    assert_eq!(unwrap_in_place(&kek, &mut buf), Err(WrapRefusal::Integrity));
    // A different KEK is caught too.
    let mut buf = [0u8; 24];
    buf[8..].copy_from_slice(&[9u8; 16]);
    wrap_in_place(&kek, &mut buf).unwrap();
    assert_eq!(unwrap_in_place(&[8u8; 16], &mut buf), Err(WrapRefusal::Integrity));
}

#[test]
fn cmac_rfc4493_vectors() {
    let key: [u8; 16] = hex("2b7e151628aed2a6abf7158809cf4f3c").try_into().unwrap();
    let m = hex(
        "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
    );
    assert_eq!(aes_cmac(&key, &[&m[..0]])[..], hex("bb1d6929e95937287fa37d129b756746")[..]);
    assert_eq!(aes_cmac(&key, &[&m[..16]])[..], hex("070a16b46b4d4144f79bdd9dd04a287c")[..]);
    assert_eq!(aes_cmac(&key, &[&m[..40]])[..], hex("dfa66747de9ae63030ca32611497c827")[..]);
    assert_eq!(aes_cmac(&key, &[&m[..64]])[..], hex("51f0bebf7e3b9d92fc49741779363cfe")[..]);
    // Parts are one message: splitting must not change the tag.
    assert_eq!(aes_cmac(&key, &[&m[..7], &m[7..33], &m[33..64]])[..], hex("51f0bebf7e3b9d92fc49741779363cfe")[..]);
}

/// ORACLE: every length class around the block boundary.
#[test]
fn cmac_boundary_lengths() {
    let key: [u8; 16] = core::array::from_fn(|i| i as u8);
    let m: Vec<u8> = (0..33u8).collect();
    for (n, want) in [
        (1usize, "d78a1663af870e5aeb4d44875eeffdf3"),
        (15, "40fb69919e3fc3f445a34234d650a72b"),
        (16, "7bcfbbca7a2ea68b966fc5399f74809e"),
        (17, "dbab59423fbec5a7be32c48ce1a80e33"),
        (32, "73dbfaf8321b282a162d2bac929fe7e7"),
        (33, "9e25b0abc7ab4433c6996e7c653bed24"),
    ] {
        assert_eq!(aes_cmac(&key, &[&m[..n]])[..], hex(want)[..], "len {n}");
    }
    assert_eq!(aes_cmac(&key, &[])[..], hex("97dd6e5a882cbd564c39ae7d1c5a31aa")[..]);
}

/// ORACLE: a 121-byte frame (the length of an EAPOL-Key with 22 bytes of key data).
#[test]
fn eapol_mic_both_versions() {
    let kck: [u8; 16] = core::array::from_fn(|i| i as u8);
    let mut frame: Vec<u8> = (0..121u8).collect();
    let off = 81;
    let z = [0u8; 16];
    frame[off..off + 16].copy_from_slice(&z);
    let m2 = mic::compute(2, &kck, &frame).unwrap();
    let m3 = mic::compute(3, &kck, &frame).unwrap();
    // verify() recomputes over the received bytes with the MIC field zeroed, so a frame stamped
    // with compute()'s output over the zeroed frame verifies.
    let mut f2 = frame.clone();
    f2[off..off + 16].copy_from_slice(&m2);
    assert_eq!(mic::verify(2, &kck, &f2, off), Ok(()));
    let mut f3 = frame.clone();
    f3[off..off + 16].copy_from_slice(&m3);
    assert_eq!(mic::verify(3, &kck, &f3, off), Ok(()));
    assert_ne!(m2, m3);
}

#[test]
fn eapol_mic_matches_plain_hmac_over_the_zeroed_frame() {
    let kck: [u8; 16] = core::array::from_fn(|i| i as u8);
    let frame: Vec<u8> = (0..121u8).collect();
    // Version 2 is HMAC-SHA1 truncated to 16 bytes. ORACLE for the unzeroed range(121) input.
    assert_eq!(mic::compute(2, &kck, &frame).unwrap()[..], hex("901f203a3054d3312f1081b3690cf343")[..]);
    assert_eq!(mic::compute(3, &kck, &frame).unwrap()[..], hex("69121fb8f1179e5a2aa726420b5dac3e")[..]);
}

#[test]
fn mic_refusals_are_named() {
    let kck = [1u8; 16];
    let frame = [0u8; 100];
    for v in [0u8, 1, 4, 7] {
        assert_eq!(mic::compute(v, &kck, &frame), Err(mic::MicRefusal::UnsupportedVersion(v)));
        assert_eq!(mic::verify(v, &kck, &frame, 81), Err(mic::MicRefusal::UnsupportedVersion(v)));
    }
    assert_eq!(mic::verify(2, &kck, &frame, 90), Err(mic::MicRefusal::Mismatch)); // field runs past the end
    let mut f = [0u8; 100];
    let good = mic::compute(2, &kck, &f).unwrap();
    f[81..97].copy_from_slice(&good);
    assert_eq!(mic::verify(2, &kck, &f, 81), Ok(()));
    f[81] ^= 1;
    assert_eq!(mic::verify(2, &kck, &f, 81), Err(mic::MicRefusal::Mismatch));
    assert_eq!(mic::verify(2, &[2u8; 16], &f, 81), Err(mic::MicRefusal::Mismatch));
}

#[test]
fn ct_eq_semantics() {
    assert!(ct_eq(b"abc", b"abc"));
    assert!(!ct_eq(b"abc", b"abd"));
    assert!(!ct_eq(b"abc", b"ab"));
}
