// SPDX-License-Identifier: GPL-2.0-only
//! DDI buffer translation tables vs intel_ddi_buf_trans.c (v7.0-rc6). Every row of every table is written out below as a Linux
//! literal (`{ .hsw = { trans1, trans2, i_boost } }`), AND the whole table is re-parsed at test time from the pinned Linux
//! source when it is on this host, so a transcription slip in either copy is caught by the other.
use i915_devinfo_core::{lookup, Platform, SubPlatform};
use i915_display_link_core::buf_trans::*;
use i915_kbl_display_core::Ddi;

type Row = (u32, u32, u8);

// _skl_trans_dp, intel_ddi_buf_trans.c:147
const SKL_DP: [Row; 9] = [
    (0x00002016, 0x000000A0, 0x0), // :148
    (0x00005012, 0x0000009B, 0x0), // :149
    (0x00007011, 0x00000088, 0x0), // :150
    (0x80009010, 0x000000C0, 0x1), // :151
    (0x00002016, 0x0000009B, 0x0), // :152
    (0x00005012, 0x00000088, 0x0), // :153
    (0x80007011, 0x000000C0, 0x1), // :154
    (0x00002016, 0x000000DF, 0x0), // :155
    (0x80005012, 0x000000C0, 0x1), // :156
];

// _skl_u_trans_dp, intel_ddi_buf_trans.c:165
const SKL_U_DP: [Row; 9] = [
    (0x0000201B, 0x000000A2, 0x0), // :166
    (0x00005012, 0x00000088, 0x0), // :167
    (0x80007011, 0x000000CD, 0x1), // :168
    (0x80009010, 0x000000C0, 0x1), // :169
    (0x0000201B, 0x0000009D, 0x0), // :170
    (0x80005012, 0x000000C0, 0x1), // :171
    (0x80007011, 0x000000C0, 0x1), // :172
    (0x00002016, 0x00000088, 0x0), // :173
    (0x80005012, 0x000000C0, 0x1), // :174
];

// _skl_y_trans_dp, intel_ddi_buf_trans.c:183
const SKL_Y_DP: [Row; 9] = [
    (0x00000018, 0x000000A2, 0x0), // :184
    (0x00005012, 0x00000088, 0x0), // :185
    (0x80007011, 0x000000CD, 0x3), // :186
    (0x80009010, 0x000000C0, 0x3), // :187
    (0x00000018, 0x0000009D, 0x0), // :188
    (0x80005012, 0x000000C0, 0x3), // :189
    (0x80007011, 0x000000C0, 0x3), // :190
    (0x00000018, 0x00000088, 0x0), // :191
    (0x80005012, 0x000000C0, 0x3), // :192
];

// _kbl_trans_dp, intel_ddi_buf_trans.c:201
const KBL_DP: [Row; 9] = [
    (0x00002016, 0x000000A0, 0x0), // :202
    (0x00005012, 0x0000009B, 0x0), // :203
    (0x00007011, 0x00000088, 0x0), // :204
    (0x80009010, 0x000000C0, 0x1), // :205
    (0x00002016, 0x0000009B, 0x0), // :206
    (0x00005012, 0x00000088, 0x0), // :207
    (0x80007011, 0x000000C0, 0x1), // :208
    (0x00002016, 0x00000097, 0x0), // :209
    (0x80005012, 0x000000C0, 0x1), // :210
];

// _kbl_u_trans_dp, intel_ddi_buf_trans.c:219
const KBL_U_DP: [Row; 9] = [
    (0x0000201B, 0x000000A1, 0x0), // :220
    (0x00005012, 0x00000088, 0x0), // :221
    (0x80007011, 0x000000CD, 0x3), // :222
    (0x80009010, 0x000000C0, 0x3), // :223
    (0x0000201B, 0x0000009D, 0x0), // :224
    (0x80005012, 0x000000C0, 0x3), // :225
    (0x80007011, 0x000000C0, 0x3), // :226
    (0x00002016, 0x0000004F, 0x0), // :227
    (0x80005012, 0x000000C0, 0x3), // :228
];

// _kbl_y_trans_dp, intel_ddi_buf_trans.c:237
const KBL_Y_DP: [Row; 9] = [
    (0x00001017, 0x000000A1, 0x0), // :238
    (0x00005012, 0x00000088, 0x0), // :239
    (0x80007011, 0x000000CD, 0x3), // :240
    (0x8000800F, 0x000000C0, 0x3), // :241
    (0x00001017, 0x0000009D, 0x0), // :242
    (0x80005012, 0x000000C0, 0x3), // :243
    (0x80007011, 0x000000C0, 0x3), // :244
    (0x00001017, 0x0000004C, 0x0), // :245
    (0x80005012, 0x000000C0, 0x3), // :246
];

// _skl_trans_edp, intel_ddi_buf_trans.c:258
const EDP: [Row; 10] = [
    (0x00000018, 0x000000A8, 0x0), // :259
    (0x00004013, 0x000000A9, 0x0), // :260
    (0x00007011, 0x000000A2, 0x0), // :261
    (0x00009010, 0x0000009C, 0x0), // :262
    (0x00000018, 0x000000A9, 0x0), // :263
    (0x00006013, 0x000000A2, 0x0), // :264
    (0x00007011, 0x000000A6, 0x0), // :265
    (0x00000018, 0x000000AB, 0x0), // :266
    (0x00007013, 0x0000009F, 0x0), // :267
    (0x00000018, 0x000000DF, 0x0), // :268
];

// _skl_u_trans_edp, intel_ddi_buf_trans.c:280
const EDP_U: [Row; 10] = [
    (0x00000018, 0x000000A8, 0x0), // :281
    (0x00004013, 0x000000A9, 0x0), // :282
    (0x00007011, 0x000000A2, 0x0), // :283
    (0x00009010, 0x0000009C, 0x0), // :284
    (0x00000018, 0x000000A9, 0x0), // :285
    (0x00006013, 0x000000A2, 0x0), // :286
    (0x00007011, 0x000000A6, 0x0), // :287
    (0x00002016, 0x000000AB, 0x0), // :288
    (0x00005013, 0x0000009F, 0x0), // :289
    (0x00000018, 0x000000DF, 0x0), // :290
];

// _skl_y_trans_edp, intel_ddi_buf_trans.c:302
const EDP_Y: [Row; 10] = [
    (0x00000018, 0x000000A8, 0x0), // :303
    (0x00004013, 0x000000AB, 0x0), // :304
    (0x00007011, 0x000000A4, 0x0), // :305
    (0x00009010, 0x000000DF, 0x0), // :306
    (0x00000018, 0x000000AA, 0x0), // :307
    (0x00006013, 0x000000A4, 0x0), // :308
    (0x00007011, 0x0000009D, 0x0), // :309
    (0x00000018, 0x000000A0, 0x0), // :310
    (0x00006012, 0x000000DF, 0x0), // :311
    (0x00000018, 0x0000008A, 0x0), // :312
];

fn rows(t: &[HswBufTrans]) -> Vec<Row> {
    t.iter().map(|e| (e.trans1, e.trans2, e.i_boost)).collect()
}

#[test]
fn every_row_of_every_table_is_the_linux_literal() {
    assert_eq!(rows(&SKL_TRANS_DP), SKL_DP.to_vec());
    assert_eq!(rows(&SKL_U_TRANS_DP), SKL_U_DP.to_vec());
    assert_eq!(rows(&SKL_Y_TRANS_DP), SKL_Y_DP.to_vec());
    assert_eq!(rows(&KBL_TRANS_DP), KBL_DP.to_vec());
    assert_eq!(rows(&KBL_U_TRANS_DP), KBL_U_DP.to_vec());
    assert_eq!(rows(&KBL_Y_TRANS_DP), KBL_Y_DP.to_vec());
    assert_eq!(rows(&SKL_TRANS_EDP), EDP.to_vec());
    assert_eq!(rows(&SKL_U_TRANS_EDP), EDP_U.to_vec());
    assert_eq!(rows(&SKL_Y_TRANS_EDP), EDP_Y.to_vec());
}

#[test]
fn entry_counts_are_nine_for_dp_and_ten_for_edp() {
    for t in [&SKL_TRANS_DP[..], &SKL_U_TRANS_DP, &SKL_Y_TRANS_DP, &KBL_TRANS_DP, &KBL_U_TRANS_DP, &KBL_Y_TRANS_DP] {
        assert_eq!(t.len(), 9);
    }
    for t in [&SKL_TRANS_EDP[..], &SKL_U_TRANS_EDP, &SKL_Y_TRANS_EDP] {
        assert_eq!(t.len(), 10);
    }
}

/// An independent reader: scan the Linux source for `{ .hsw = { A, B, C } }` rows after the array's declaration.
fn linux_rows(array: &str) -> Option<Vec<Row>> {
    let path = std::env::var("I915_LINUX_REF").unwrap_or_else(|_| "/home/peter/Projects/references/coralos/linux-ref".into());
    let src = std::fs::read_to_string(format!("{path}/drivers/gpu/drm/i915/display/intel_ddi_buf_trans.c")).ok()?;
    let start = src.find(&format!("intel_ddi_buf_trans_entry {array}[]"))?;
    let body = &src[start..];
    let end = body.find("};")?;
    let mut out = vec![];
    for chunk in body[..end].split("{ .hsw = {").skip(1) {
        let inner = chunk.split('}').next()?;
        let v: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        let p = |s: &str| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap();
        out.push((p(v[0]), p(v[1]), p(v[2]) as u8));
    }
    Some(out)
}

#[test]
fn the_tables_equal_the_pinned_linux_source_when_it_is_present() {
    let pairs: [(&str, &[HswBufTrans]); 9] = [
        ("_skl_trans_dp", &SKL_TRANS_DP),
        ("_skl_u_trans_dp", &SKL_U_TRANS_DP),
        ("_skl_y_trans_dp", &SKL_Y_TRANS_DP),
        ("_kbl_trans_dp", &KBL_TRANS_DP),
        ("_kbl_u_trans_dp", &KBL_U_TRANS_DP),
        ("_kbl_y_trans_dp", &KBL_Y_TRANS_DP),
        ("_skl_trans_edp", &SKL_TRANS_EDP),
        ("_skl_u_trans_edp", &SKL_U_TRANS_EDP),
        ("_skl_y_trans_edp", &SKL_Y_TRANS_EDP),
    ];
    let mut checked = 0;
    for (name, table) in pairs {
        if let Some(l) = linux_rows(name) {
            assert_eq!(rows(table), l, "{name}");
            checked += 1;
        }
    }
    if checked == 0 {
        eprintln!("SKIP: Linux source not on this host (set I915_LINUX_REF)");
    } else {
        assert_eq!(checked, 9);
    }
}

#[test]
fn i_boost_is_always_a_valid_skl_value() {
    // intel_ddi_buf_trans.h:16 "valid: 0x0, 0x1, 0x3, 0x7"
    for t in [&SKL_TRANS_DP[..], &SKL_U_TRANS_DP, &SKL_Y_TRANS_DP, &KBL_TRANS_DP, &KBL_U_TRANS_DP, &KBL_Y_TRANS_DP, &SKL_TRANS_EDP, &SKL_U_TRANS_EDP, &SKL_Y_TRANS_EDP] {
        for e in t {
            assert!([0, 1, 3, 7].contains(&e.i_boost));
        }
    }
}

// ---- selection ---------------------------------------------------------------------------------------------------
fn sku_of(devid: u16) -> Sku {
    Sku::from_descriptor(&lookup(devid).unwrap()).unwrap()
}

#[test]
fn the_pixelbook_go_is_a_kaby_lake_y_buffer() {
    // 8086:591C is Kaby Lake ULX (pciids.h:455 INTEL_AML_KBL_GT2_IDS); kabylake_ulx -> kbl_y_get_buf_trans (:1827-1830)
    assert_eq!(sku_of(0x591c), Sku::KblY);
    assert_eq!(sku_of(0x5916), Sku::KblU); // KBL ULT GT2
    assert_eq!(sku_of(0x5912), Sku::KblHs); // KBL DT GT2
    assert_eq!(sku_of(0x191e), Sku::SklY); // SKL ULX GT2
    assert_eq!(sku_of(0x1916), Sku::SklU);
    assert_eq!(sku_of(0x1912), Sku::SklHs);
    // Cherryview (Gen8) has no such table: named refusal, not a fallthrough
    let chv = lookup(0x22b0).unwrap();
    assert_eq!(chv.platform(), Platform::Cherryview);
    let e = Sku::from_descriptor(&chv).unwrap_err();
    assert_eq!(e, BufTransError::UnsupportedPlatform { devid: 0x22b0 });
    assert_eq!(e.name(), "BUF-TRANS-UNSUPPORTED-PLATFORM");
    let _ = SubPlatform::None;
}

#[test]
fn edp_low_vswing_picks_the_shared_skl_edp_table_of_the_sku_else_the_dp_table() {
    // kbl_y_get_buf_trans, :1300-1312
    assert_eq!(get_buf_trans(Sku::KblY, true, true, Ddi::A).entries.as_ptr(), SKL_Y_TRANS_EDP.as_ptr());
    assert_eq!(get_buf_trans(Sku::KblY, true, false, Ddi::A).entries.as_ptr(), KBL_Y_TRANS_DP.as_ptr());
    assert_eq!(get_buf_trans(Sku::KblY, false, true, Ddi::A).entries.as_ptr(), KBL_Y_TRANS_DP.as_ptr()); // low_vswing only counts for eDP
    assert_eq!(get_buf_trans(Sku::KblU, true, true, Ddi::A).entries.as_ptr(), SKL_U_TRANS_EDP.as_ptr());
    assert_eq!(get_buf_trans(Sku::KblU, true, false, Ddi::A).entries.as_ptr(), KBL_U_TRANS_DP.as_ptr());
    assert_eq!(get_buf_trans(Sku::KblHs, true, true, Ddi::A).entries.as_ptr(), SKL_TRANS_EDP.as_ptr());
    assert_eq!(get_buf_trans(Sku::KblHs, true, false, Ddi::A).entries.as_ptr(), KBL_TRANS_DP.as_ptr());
    // skl_*_get_buf_trans, :1258-1297
    assert_eq!(get_buf_trans(Sku::SklY, true, true, Ddi::A).entries.as_ptr(), SKL_Y_TRANS_EDP.as_ptr());
    assert_eq!(get_buf_trans(Sku::SklY, false, false, Ddi::A).entries.as_ptr(), SKL_Y_TRANS_DP.as_ptr());
    assert_eq!(get_buf_trans(Sku::SklU, true, true, Ddi::A).entries.as_ptr(), SKL_U_TRANS_EDP.as_ptr());
    assert_eq!(get_buf_trans(Sku::SklU, false, false, Ddi::A).entries.as_ptr(), SKL_U_TRANS_DP.as_ptr());
    assert_eq!(get_buf_trans(Sku::SklHs, true, true, Ddi::A).entries.as_ptr(), SKL_TRANS_EDP.as_ptr());
    assert_eq!(get_buf_trans(Sku::SklHs, false, false, Ddi::A).entries.as_ptr(), SKL_TRANS_DP.as_ptr());
}

#[test]
fn only_ddi_a_and_e_may_use_the_tenth_row() {
    // skl_buf_trans_num_entries, :1239-1246
    for port in [Ddi::A, Ddi::E] {
        assert_eq!(get_buf_trans(Sku::KblY, true, true, port).n_entries, 10);
        assert_eq!(get_buf_trans(Sku::KblY, false, false, port).n_entries, 9);
    }
    for port in [Ddi::B, Ddi::C, Ddi::D] {
        assert_eq!(get_buf_trans(Sku::KblY, true, true, port).n_entries, 9);
        assert_eq!(get_buf_trans(Sku::KblY, false, false, port).n_entries, 9);
    }
    assert_eq!(skl_buf_trans_num_entries(Ddi::A, 12), 10);
    assert_eq!(skl_buf_trans_num_entries(Ddi::B, 12), 9);
    assert_eq!(skl_buf_trans_num_entries(Ddi::A, 4), 4);
}

#[test]
fn levels_are_clamped_or_refused_by_name() {
    let t = get_buf_trans(Sku::KblY, false, false, Ddi::A); // 9 entries
    assert_eq!(t.clamp_level(3), 3);
    assert_eq!(t.clamp_level(8), 8);
    assert_eq!(t.clamp_level(9), 8); // intel_ddi.c:1524-1527
    assert_eq!(t.clamp_level(100), 8);
    assert_eq!(t.entry(8).unwrap(), KBL_Y_TRANS_DP[8]);
    let e = t.entry(9).unwrap_err();
    assert_eq!(e, BufTransError::NoSuchEntry { level: 9, n_entries: 9 });
    assert_eq!(e.name(), "BUF-TRANS-NO-SUCH-ENTRY");
}

#[test]
fn spot_rows_of_the_atlas_tables() {
    // The Pixelbook (KBL-Y, eDP low vswing, VBT low_vswing = true): skl_y_trans_edp, intel_ddi_buf_trans.c:303-312
    let t = get_buf_trans(Sku::KblY, true, true, Ddi::A);
    assert_eq!(t.entries[0], HswBufTrans { trans1: 0x0000_0018, trans2: 0x0000_00A8, i_boost: 0 });
    assert_eq!(t.entries[3], HswBufTrans { trans1: 0x0000_9010, trans2: 0x0000_00DF, i_boost: 0 });
    assert_eq!(t.entries[8], HswBufTrans { trans1: 0x0000_6012, trans2: 0x0000_00DF, i_boost: 0 });
    assert_eq!(t.entries[9], HswBufTrans { trans1: 0x0000_0018, trans2: 0x0000_008A, i_boost: 0 });
    // kbl_y_trans_dp (:238-246): the I_boost rows carry 0x3, and row 3 has the balance-leg bit and 0x8000800F
    let d = get_buf_trans(Sku::KblY, false, false, Ddi::A);
    assert_eq!(d.entries[3], HswBufTrans { trans1: 0x8000_800F, trans2: 0x0000_00C0, i_boost: 3 });
    assert_eq!(d.entries[7], HswBufTrans { trans1: 0x0000_1017, trans2: 0x0000_004C, i_boost: 0 });
}
