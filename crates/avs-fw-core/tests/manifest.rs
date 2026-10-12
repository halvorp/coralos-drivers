// SPDX-License-Identifier: GPL-2.0-only
//! avs_fw_manifest_strip_verify (loader.c:102-136) and its helpers, on synthetic images.
mod common;
use avs_fw_core::error::*;
use avs_fw_core::manifest::*;
use common::skl_image;

const NAME: &[u8; 8] = b"ADSPFW\0\0";

fn v(a: u16, b: u16, c: u16, d: u16) -> FwVersion {
    FwVersion { major: a, minor: b, hotfix: c, build: d }
}

#[test]
fn strips_the_extended_manifest_and_finds_the_manifest_at_0x284() {
    let img = skl_image(Some(0x548), NAME, (9, 21, 0, 4899), 16);
    let s = strip_verify(&img, ManifestFamily::Skl, Some(skl_min_version()), VersionPolicy::Enforce).unwrap();
    assert_eq!(s.stripped, 0x548);
    assert_eq!(s.body.len(), img.len() - 0x548);
    assert_eq!(s.family, ManifestFamily::Skl);
    assert_eq!(s.manifest_offset, 0x284);
    let e = s.ext.unwrap();
    assert_eq!((e.id, e.len, e.version_major, e.version_minor, e.entries), (0x31454124, 0x548, 1, 2, 3));
    assert_eq!(s.manifest.id, 0x314d4124);
    assert_eq!(s.manifest.len, 0x34);
    assert_eq!(&s.manifest.name, NAME);
    assert_eq!(s.manifest.preload_page_count, 0x3b);
    assert_eq!(s.manifest.img_flags, 0x5);
    assert_eq!(s.manifest.feature_mask, 0x1f);
    assert_eq!(s.manifest.version, v(9, 21, 0, 4899));
    assert_eq!(s.version_ignored, None);
}

#[test]
fn an_image_without_an_extended_manifest_is_not_stripped() {
    let img = skl_image(None, NAME, (9, 21, 0, 4732), 0);
    let s = strip_verify(&img, ManifestFamily::Skl, Some(skl_min_version()), VersionPolicy::Enforce).unwrap();
    assert_eq!(s.ext, None);
    assert_eq!(s.stripped, 0);
    assert_eq!(s.body.len(), img.len());
}

#[test]
fn version_gate_requires_equal_major_minor_hotfix_and_build_at_least_the_minimum() {
    // loader.c:122-125. min = 9.21.0.4732 (core.c:817)
    let cases: [((u16, u16, u16, u16), bool); 8] = [
        ((9, 21, 0, 4732), true),
        ((9, 21, 0, 4733), true),
        ((9, 21, 0, 4731), false),
        ((9, 21, 1, 4899), false),
        ((9, 22, 0, 4899), false),
        ((9, 20, 0, 4899), false),
        ((10, 21, 0, 4899), false),
        ((8, 21, 0, 4899), false),
    ];
    for (ver, ok) in cases {
        let img = skl_image(Some(0x20), NAME, ver, 0);
        let r = strip_verify(&img, ManifestFamily::Skl, Some(skl_min_version()), VersionPolicy::Enforce);
        match (ok, r) {
            (true, Ok(_)) => {}
            (false, Err(FwError::FwVersionMismatch { found, min })) => {
                assert_eq!(found, v(ver.0, ver.1, ver.2, ver.3));
                assert_eq!(min, v(9, 21, 0, 4732));
            }
            (ok, r) => panic!("{ver:?}: expected ok={ok}, got {r:?}"),
        }
    }
}

#[test]
fn ignore_policy_reports_the_mismatch_instead_of_refusing() {
    // loader.c:44-47, :131-133 (`ignore_fw_version`)
    let img = skl_image(Some(0x20), NAME, (9, 21, 0, 100), 0);
    let s = strip_verify(&img, ManifestFamily::Skl, Some(skl_min_version()), VersionPolicy::Ignore).unwrap();
    assert_eq!(s.version_ignored, Some(v(9, 21, 0, 100)));
}

#[test]
fn no_minimum_means_no_version_gate() {
    // libraries are verified with min == NULL (loader.c:525, :118-119)
    let img = skl_image(Some(0x20), NAME, (1, 2, 3, 4), 0);
    let s = strip_verify(&img, ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap();
    assert_eq!(s.manifest.version, v(1, 2, 3, 4));
}

#[test]
fn extended_manifest_longer_than_the_image_is_refused_by_name() {
    let mut img = skl_image(Some(0x20), NAME, (9, 21, 0, 4899), 0);
    img[4..8].copy_from_slice(&0x10_0000u32.to_le_bytes());
    let e = strip_verify(&img, ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap_err();
    assert_eq!(e, FwError::ExtManifestPastImage { ext_len: 0x10_0000, image_len: img.len() });
    assert_eq!(e.name(), "ext-manifest-past-image");
}

#[test]
fn extended_manifest_length_exactly_the_image_leaves_no_body() {
    let mut img = skl_image(Some(0x20), NAME, (9, 21, 0, 4899), 0);
    let n = img.len() as u32;
    img[4..8].copy_from_slice(&n.to_le_bytes());
    let e = strip_verify(&img, ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap_err();
    assert_eq!(e, FwError::ImageTooShort { have: 0, need: 4 });
}

#[test]
fn extended_manifest_shorter_than_its_own_header_is_refused_by_name() {
    for l in [0u32, 8, 15] {
        let mut img = skl_image(Some(0x20), NAME, (9, 21, 0, 4899), 0);
        img[4..8].copy_from_slice(&l.to_le_bytes());
        let e = strip_verify(&img, ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap_err();
        assert_eq!(e, FwError::ExtManifestShorterThanHeader { ext_len: l });
    }
    // 16 exactly is the smallest legal one
    let mut img = skl_image(Some(0x20), NAME, (9, 21, 0, 4899), 0);
    img[4..8].copy_from_slice(&16u32.to_le_bytes());
    // the body then starts inside the old ext block (zeros): unknown magic, NOT the length refusal
    assert!(matches!(strip_verify(&img, ManifestFamily::Skl, None, VersionPolicy::Enforce), Err(FwError::UnknownManifestMagic { magic: 0 })));
}

#[test]
fn short_images_are_refused_at_each_read() {
    // loader.c:75-76: fewer than 16 bytes cannot be examined for the extended manifest at all
    for n in [0usize, 1, 15] {
        let e = strip_verify(&vec![0u8; n], ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap_err();
        assert_eq!(e, FwError::ImageTooShort { have: n, need: 16 });
    }
    // manifest cut one byte short of its 36 bytes (loader.c:116-117)
    let img = skl_image(None, NAME, (9, 21, 0, 4899), 0);
    let cut = &img[..img.len() - 1];
    let e = strip_verify(cut, ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap_err();
    assert_eq!(e, FwError::ImageTooShort { have: 0x284 + 35, need: 0x284 + 36 });
    // and exactly 36 bytes is enough
    assert!(strip_verify(&img, ManifestFamily::Skl, None, VersionPolicy::Enforce).is_ok());
}

#[test]
fn unknown_magic_is_refused_with_the_magic() {
    let mut img = skl_image(None, NAME, (9, 21, 0, 4899), 0);
    img[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
    let e = strip_verify(&img, ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap_err();
    assert_eq!(e, FwError::UnknownManifestMagic { magic: 0xDEADBEEF });
}

#[test]
fn an_apollo_lake_image_is_refused_for_the_kaby_lake_loader() {
    // loader.c:95-96 knows the APL magic and offset; the KBL row must not take it
    let mut img = vec![0u8; 0x2000 + 64];
    img[0..4].copy_from_slice(&0x44504324u32.to_le_bytes());
    assert_eq!(manifest_family(&img).unwrap(), (ManifestFamily::Apl, 0x2000));
    let e = strip_verify(&img, ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap_err();
    assert_eq!(e, FwError::ManifestFamilyMismatch { found: ManifestFamily::Apl, expected: ManifestFamily::Skl });
    // the same bytes ARE valid for the family they belong to
    assert!(strip_verify(&img, ManifestFamily::Apl, None, VersionPolicy::Enforce).is_ok());
}

#[test]
fn family_table_is_exactly_two_magics() {
    // loader.c:92-99: Skylake 6 -> 0x284, Apollo Lake 0x44504324 -> 0x2000, everything else EINVAL
    let mut b = [0u8; 8];
    b[0..4].copy_from_slice(&6u32.to_le_bytes());
    assert_eq!(manifest_family(&b).unwrap(), (ManifestFamily::Skl, 0x284));
    b[0..4].copy_from_slice(&0x44504324u32.to_le_bytes());
    assert_eq!(manifest_family(&b).unwrap(), (ManifestFamily::Apl, 0x2000));
    for m in [0u32, 5, 7, 0x31454124, 0x314d4124] {
        b[0..4].copy_from_slice(&m.to_le_bytes());
        assert_eq!(manifest_family(&b), Err(FwError::UnknownManifestMagic { magic: m }));
    }
    assert_eq!(manifest_family(&b[..3]), Err(FwError::ImageTooShort { have: 3, need: 4 }));
}

#[test]
fn ext_manifest_is_recognised_by_its_magic_only() {
    let img = skl_image(None, NAME, (9, 21, 0, 4899), 0);
    assert_eq!(parse_ext_manifest(&img).unwrap(), None);
}

#[test]
fn min_version_is_the_skylake_row() {
    assert_eq!(skl_min_version(), v(9, 21, 0, 4732)); // core.c:817
}

#[test]
fn exactly_sixteen_bytes_is_enough_to_look_for_an_extended_manifest() {
    // loader.c:75 refuses `fw->size < sizeof(*man)`: 16 bytes is NOT too short
    assert_eq!(parse_ext_manifest(&[0u8; 16]), Ok(None));
    assert_eq!(parse_ext_manifest(&[0u8; 15]), Err(FwError::ImageTooShort { have: 15, need: 16 }));
}

#[test]
fn four_bytes_are_enough_to_read_the_magic() {
    assert_eq!(manifest_family(&6u32.to_le_bytes()), Ok((ManifestFamily::Skl, 0x284)));
}
