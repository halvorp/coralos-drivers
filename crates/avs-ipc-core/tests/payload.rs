// SPDX-License-Identifier: GPL-2.0-only
//! The firmware/hardware configuration TLVs and the modules-info table (messages.c:391-661, messages.h:424-601).
mod common;
use avs_ipc_core::error::IpcError;
use avs_ipc_core::payload::*;
use common::{dw, tlv};

fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.iter().flatten().copied().collect()
}

#[test]
fn tlv_walk_follows_length_with_no_alignment() {
    let p = cat(&[tlv(1, &[1, 2, 3]), tlv(2, &[]), tlv(3, &[9; 8])]);
    let v: Vec<_> = tlvs(&p).map(|t| t.unwrap()).collect();
    assert_eq!(v.len(), 3);
    assert_eq!((v[0].ty, v[0].value), (1, &[1u8, 2, 3][..]));
    assert_eq!((v[1].ty, v[1].value.len()), (2, 0));
    assert_eq!((v[2].ty, v[2].value.len()), (3, 8));
}

#[test]
fn a_cut_short_tlv_is_a_named_truncation_not_an_overrun() {
    // header cut: 7 bytes left
    let mut p = tlv(1, &[1, 2, 3, 4]);
    p.extend_from_slice(&[0; 7]);
    let r: Vec<_> = tlvs(&p).collect();
    assert_eq!(r.len(), 2);
    assert!(r[0].is_ok());
    assert_eq!(r[1], Err(IpcError::Truncated { what: "tlv header", need: 8, have: 7 }));
    // value cut: header says 16, 15 follow
    let mut p = tlv(1, &[0; 16]);
    p.pop();
    let r: Vec<_> = tlvs(&p).collect();
    assert_eq!(r, vec![Err(IpcError::Truncated { what: "tlv value", need: 16, have: 15 })]);
    // an absurd length does not overflow
    let mut p = Vec::new();
    p.extend_from_slice(&dw(1));
    p.extend_from_slice(&dw(u32::MAX));
    assert!(matches!(tlvs(&p).next(), Some(Err(IpcError::Truncated { what: "tlv value", .. }))));
    // exactly fitting is fine, and the walk stops after an error
    let p = tlv(1, &[0; 16]);
    assert_eq!(tlvs(&p).count(), 1);
}

#[test]
fn tlv_dword_refuses_a_value_that_is_too_short() {
    let t = Tlv { ty: 9, value: &[1, 2, 3, 4, 5, 6, 7] };
    assert_eq!(t.dword(0), Ok(0x04030201));
    assert_eq!(t.dword(1), Err(IpcError::TlvValueTooShort { tlv_type: 9, need: 8, have: 7 }));
}

#[test]
fn fw_config_parses_every_scalar_it_knows() {
    // one TLV per known type (messages.c:410-490), each with a distinct value so a swapped case is visible
    let mut parts = vec![tlv(AVS_FW_CFG_FW_VERSION, &[9, 0, 21, 0, 0, 0, 0x23, 0x13])];
    let scalars: [(u32, u32); 17] = [
        (AVS_FW_CFG_MEMORY_RECLAIMED, 0x11),
        (AVS_FW_CFG_SLOW_CLOCK_FREQ_HZ, 0x12),
        (AVS_FW_CFG_FAST_CLOCK_FREQ_HZ, 0x13),
        (AVS_FW_CFG_ALH_SUPPORT_LEVEL, 0x14),
        (AVS_FW_CFG_IPC_DL_MAILBOX_BYTES, 0x15),
        (AVS_FW_CFG_IPC_UL_MAILBOX_BYTES, 0x16),
        (AVS_FW_CFG_TRACE_LOG_BYTES, 0x17),
        (AVS_FW_CFG_MAX_PPL_COUNT, 0x18),
        (AVS_FW_CFG_MAX_ASTATE_COUNT, 0x19),
        (AVS_FW_CFG_MAX_MODULE_PIN_COUNT, 0x1A),
        (AVS_FW_CFG_MODULES_COUNT, 0x1B),
        (AVS_FW_CFG_MAX_MOD_INST_COUNT, 0x1C),
        (AVS_FW_CFG_MAX_LL_TASKS_PER_PRI_COUNT, 0x1D),
        (AVS_FW_CFG_LL_PRI_COUNT, 0x1E),
        (AVS_FW_CFG_MAX_DP_TASKS_COUNT, 0x1F),
        (AVS_FW_CFG_MAX_LIBS_COUNT, 0x20),
        (AVS_FW_CFG_XTAL_FREQ_HZ, 0x21),
    ];
    for (t, v) in scalars {
        parts.push(tlv(t, &dw(v)));
    }
    parts.push(tlv(AVS_FW_CFG_POWER_GATING_POLICY, &dw(0x22)));
    // known-but-ignored and unknown types
    for t in [AVS_FW_CFG_DMA_BUFFER_CONFIG, AVS_FW_CFG_SCHEDULER_CONFIG, AVS_FW_CFG_CLOCKS_CONFIG, AVS_FW_CFG_RESERVED] {
        parts.push(tlv(t, &dw(0xDEAD)));
    }
    parts.push(tlv(AVS_FW_CFG_ASSERT_MODE, &dw(1)));
    parts.push(tlv(AVS_FW_CFG_BUS_HARDWARE_ID, &dw(1)));
    parts.push(tlv(77, &[]));
    let c = parse_fw_config(&cat(&parts)).unwrap();
    assert_eq!(c.fw_version, FwVersion { major: 9, minor: 21, hotfix: 0, build: 0x1323 });
    assert_eq!(c.memory_reclaimed, 0x11);
    assert_eq!(c.slow_clock_freq_hz, 0x12);
    assert_eq!(c.fast_clock_freq_hz, 0x13);
    assert_eq!(c.alh_support, 0x14);
    assert_eq!(c.ipc_dl_mailbox_bytes, 0x15);
    assert_eq!(c.ipc_ul_mailbox_bytes, 0x16);
    assert_eq!(c.trace_log_bytes, 0x17);
    assert_eq!(c.max_ppl_count, 0x18);
    assert_eq!(c.max_astate_count, 0x19);
    assert_eq!(c.max_module_pin_count, 0x1A);
    assert_eq!(c.modules_count, 0x1B);
    assert_eq!(c.max_mod_inst_count, 0x1C);
    assert_eq!(c.max_ll_tasks_per_pri_count, 0x1D);
    assert_eq!(c.ll_pri_count, 0x1E);
    assert_eq!(c.max_dp_tasks_count, 0x1F);
    assert_eq!(c.max_libs_count, 0x20);
    assert_eq!(c.xtal_freq_hz, 0x21);
    assert_eq!(c.power_gating_policy, 0x22);
    // ASSERT_MODE, BUS_HARDWARE_ID (not handled by Linux either) and type 77 are "unrecognized"; the four ignored ones are not
    assert_eq!(c.unrecognized, 3);
}

#[test]
fn fw_config_type_numbers() {
    let expect = [
        (AVS_FW_CFG_FW_VERSION, 0),
        (AVS_FW_CFG_MEMORY_RECLAIMED, 1),
        (AVS_FW_CFG_SLOW_CLOCK_FREQ_HZ, 2),
        (AVS_FW_CFG_FAST_CLOCK_FREQ_HZ, 3),
        (AVS_FW_CFG_DMA_BUFFER_CONFIG, 4),
        (AVS_FW_CFG_ALH_SUPPORT_LEVEL, 5),
        (AVS_FW_CFG_IPC_DL_MAILBOX_BYTES, 6),
        (AVS_FW_CFG_IPC_UL_MAILBOX_BYTES, 7),
        (AVS_FW_CFG_TRACE_LOG_BYTES, 8),
        (AVS_FW_CFG_MAX_PPL_COUNT, 9),
        (AVS_FW_CFG_MAX_ASTATE_COUNT, 10),
        (AVS_FW_CFG_MAX_MODULE_PIN_COUNT, 11),
        (AVS_FW_CFG_MODULES_COUNT, 12),
        (AVS_FW_CFG_MAX_MOD_INST_COUNT, 13),
        (AVS_FW_CFG_MAX_LL_TASKS_PER_PRI_COUNT, 14),
        (AVS_FW_CFG_LL_PRI_COUNT, 15),
        (AVS_FW_CFG_MAX_DP_TASKS_COUNT, 16),
        (AVS_FW_CFG_MAX_LIBS_COUNT, 17),
        (AVS_FW_CFG_SCHEDULER_CONFIG, 18),
        (AVS_FW_CFG_XTAL_FREQ_HZ, 19),
        (AVS_FW_CFG_CLOCKS_CONFIG, 20),
        (AVS_FW_CFG_RESERVED, 21),
        (AVS_FW_CFG_POWER_GATING_POLICY, 22),
        (AVS_FW_CFG_ASSERT_MODE, 23),
        (AVS_FW_CFG_RESERVED2, 24),
        (AVS_FW_CFG_BUS_HARDWARE_ID, 25),
    ];
    for (c, v) in expect {
        assert_eq!(c, v);
    }
}

#[test]
fn fw_config_refusals() {
    assert_eq!(parse_fw_config(&[]), Err(IpcError::EmptyPayload { what: "firmware config" }));
    // a version TLV of 6 bytes cannot hold four u16
    assert_eq!(parse_fw_config(&tlv(AVS_FW_CFG_FW_VERSION, &[0; 6])), Err(IpcError::TlvValueTooShort { tlv_type: 0, need: 8, have: 6 }));
    // a scalar TLV with a 2-byte value
    assert_eq!(parse_fw_config(&tlv(AVS_FW_CFG_MAX_LIBS_COUNT, &[1, 2])), Err(IpcError::TlvValueTooShort { tlv_type: 17, need: 4, have: 2 }));
    // a TLV cut short
    let mut p = tlv(AVS_FW_CFG_MAX_LIBS_COUNT, &dw(7));
    p.pop();
    assert!(matches!(parse_fw_config(&p), Err(IpcError::Truncated { .. })));
    // a field the firmware did not report stays zero
    let c = parse_fw_config(&tlv(AVS_FW_CFG_MAX_LIBS_COUNT, &dw(16))).unwrap();
    assert_eq!((c.max_libs_count, c.modules_count, c.unrecognized), (16, 0, 0));
}

#[test]
fn hw_config_parses_scalars_and_the_i2s_caps() {
    let mut i2s = Vec::new();
    i2s.extend_from_slice(&dw(AVS_I2S_VER_18_KBL_CNL));
    i2s.extend_from_slice(&dw(3));
    for a in [0x1000u32, 0x2000, 0x3000] {
        i2s.extend_from_slice(&dw(a));
    }
    let p = cat(&[
        tlv(AVS_HW_CFG_AVS_VER, &dw(AVS_AVS_VER_1_8)),
        tlv(AVS_HW_CFG_DSP_CORES, &dw(2)),
        tlv(AVS_HW_CFG_MEM_PAGE_BYTES, &dw(0x1000)),
        tlv(AVS_HW_CFG_TOTAL_PHYS_MEM_PAGES, &dw(0x40)),
        tlv(AVS_HW_CFG_I2S_CAPS, &i2s),
        tlv(AVS_HW_CFG_GPDMA_CAPS, &dw(0xDEAD)),
        tlv(AVS_HW_CFG_GATEWAY_COUNT, &dw(8)),
        tlv(AVS_HW_CFG_HP_EBB_COUNT, &dw(9)),
        tlv(AVS_HW_CFG_LP_EBB_COUNT, &dw(10)),
        tlv(AVS_HW_CFG_EBB_SIZE_BYTES, &dw(11)),
        tlv(40, &[]),
    ]);
    let c = parse_hw_config(&p).unwrap();
    assert_eq!(c.avs_version, 0x10008);
    assert_eq!(c.dsp_cores, 2);
    assert_eq!(c.mem_page_bytes, 0x1000);
    assert_eq!(c.total_phys_mem_pages, 0x40);
    assert_eq!(c.i2s_caps.i2s_version, 0x30000);
    assert_eq!(c.i2s_caps.ctrl_count, 3);
    assert_eq!(&c.i2s_caps.ctrl_base_addr[..4], &[0x1000, 0x2000, 0x3000, 0]);
    assert_eq!((c.gateway_count, c.hp_ebb_count, c.lp_ebb_count, c.ebb_size_bytes), (8, 9, 10, 11));
    assert_eq!(c.unrecognized, 1);
}

#[test]
fn hw_config_type_and_version_numbers() {
    let t = [
        (AVS_HW_CFG_AVS_VER, 0),
        (AVS_HW_CFG_DSP_CORES, 1),
        (AVS_HW_CFG_MEM_PAGE_BYTES, 2),
        (AVS_HW_CFG_TOTAL_PHYS_MEM_PAGES, 3),
        (AVS_HW_CFG_I2S_CAPS, 4),
        (AVS_HW_CFG_GPDMA_CAPS, 5),
        (AVS_HW_CFG_GATEWAY_COUNT, 6),
        (AVS_HW_CFG_HP_EBB_COUNT, 7),
        (AVS_HW_CFG_LP_EBB_COUNT, 8),
        (AVS_HW_CFG_EBB_SIZE_BYTES, 9),
    ];
    for (c, v) in t {
        assert_eq!(c, v);
    }
    assert_eq!((AVS_AVS_VER_1_5, AVS_AVS_VER_1_8), (0x10005, 0x10008));
    assert_eq!((AVS_I2S_VER_15_SKYLAKE, AVS_I2S_VER_15_BROXTON, AVS_I2S_VER_15_BROXTON_P, AVS_I2S_VER_18_KBL_CNL), (0, 0x10000, 0x20000, 0x30000));
}

#[test]
fn hw_config_i2s_caps_refusals() {
    // zero controllers: no array, fine
    let mut z = Vec::new();
    z.extend_from_slice(&dw(0x30000));
    z.extend_from_slice(&dw(0));
    let c = parse_hw_config(&tlv(AVS_HW_CFG_I2S_CAPS, &z)).unwrap();
    assert_eq!(c.i2s_caps.ctrl_count, 0);
    // the TLV claims 3 controllers but carries 2 addresses
    let mut s = Vec::new();
    s.extend_from_slice(&dw(0x30000));
    s.extend_from_slice(&dw(3));
    s.extend_from_slice(&dw(1));
    s.extend_from_slice(&dw(2));
    assert_eq!(parse_hw_config(&tlv(AVS_HW_CFG_I2S_CAPS, &s)), Err(IpcError::TlvValueTooShort { tlv_type: 4, need: 20, have: 16 }));
    // more controllers than the fixed storage: refused by name (the TLV itself would be long enough)
    let mut b = Vec::new();
    b.extend_from_slice(&dw(0x30000));
    b.extend_from_slice(&dw(17));
    b.extend_from_slice(&[0u8; 17 * 4]);
    assert_eq!(parse_hw_config(&tlv(AVS_HW_CFG_I2S_CAPS, &b)), Err(IpcError::TooMany { what: "i2s controllers", count: 17, max: MAX_I2S_CTRL }));
    // exactly 16 is fine
    let mut e = Vec::new();
    e.extend_from_slice(&dw(0x30000));
    e.extend_from_slice(&dw(16));
    e.extend_from_slice(&[0u8; 16 * 4]);
    assert!(parse_hw_config(&tlv(AVS_HW_CFG_I2S_CAPS, &e)).is_ok());
    assert_eq!(parse_hw_config(&[]), Err(IpcError::EmptyPayload { what: "hardware config" }));
    // an i2s TLV with only the version dword has no count to read
    assert!(matches!(parse_hw_config(&tlv(AVS_HW_CFG_I2S_CAPS, &dw(0))), Err(IpcError::TlvValueTooShort { tlv_type: 4, need: 8, have: 4 })));
}

/// One 116-byte module entry with a distinct value in every field so a wrong offset shows.
fn entry(id: u16, uuid_seed: u8) -> Vec<u8> {
    let mut e = Vec::new();
    e.extend_from_slice(&id.to_le_bytes()); // 0 module_id
    e.extend_from_slice(&0x0001u16.to_le_bytes()); // 2 state_flags
    e.extend_from_slice(b"MODNAME\0"); // 4 name
    e.extend((0..16).map(|i| uuid_seed + i as u8)); // 12 uuid
    e.extend_from_slice(&0x0000_00B1u32.to_le_bytes()); // 28 type: load_type 1, auto_start 1... see below
    e.extend((0..32).map(|i| 0xA0 + i as u8)); // 32 hash
    e.extend_from_slice(&0x1111_2222u32.to_le_bytes()); // 64 entry_point
    e.extend_from_slice(&0x3333u16.to_le_bytes()); // 68 cfg_offset
    e.extend_from_slice(&0x4444u16.to_le_bytes()); // 70 cfg_count
    e.extend_from_slice(&0x5555_6666u32.to_le_bytes()); // 72 affinity_mask
    e.extend_from_slice(&0x7777u16.to_le_bytes()); // 76 instance_max_count
    e.extend_from_slice(&0x8888u16.to_le_bytes()); // 78 instance_bss_size
    for s in 0..3u32 {
        e.extend_from_slice(&(0x0040_0000 | (s << 8) | 0x35).to_le_bytes()); // flags
        e.extend_from_slice(&(0xB000_0000 + s).to_le_bytes()); // v_base_addr
        e.extend_from_slice(&(0x100 * (s + 1)).to_le_bytes()); // file_offset
    }
    assert_eq!(e.len(), 116);
    e
}

fn modules(n: u32, extra: usize) -> Vec<u8> {
    let mut p = dw(n).to_vec();
    for i in 0..n {
        p.extend(entry(i as u16 + 1, 0x10 * (i as u8 + 1)));
    }
    p.extend(vec![0u8; extra]);
    p
}

#[test]
fn modules_info_entries_land_on_the_declared_offsets() {
    let p = modules(2, 0);
    let m = parse_modules_info(&p).unwrap();
    assert_eq!(m.count(), 2);
    let e = m.entry(0).unwrap();
    assert_eq!(e.module_id, 1);
    assert_eq!(e.state_flags, 1);
    assert_eq!(&e.name, b"MODNAME\0");
    assert_eq!(e.uuid, [0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F]);
    assert_eq!(e.type_bits, 0xB1);
    assert_eq!(e.hash[0], 0xA0);
    assert_eq!(e.hash[31], 0xBF);
    assert_eq!(e.entry_point, 0x1111_2222);
    assert_eq!(e.cfg_offset, 0x3333);
    assert_eq!(e.cfg_count, 0x4444);
    assert_eq!(e.affinity_mask, 0x5555_6666);
    assert_eq!(e.instance_max_count, 0x7777);
    assert_eq!(e.instance_bss_size, 0x8888);
    for (s, seg) in e.segments.iter().enumerate() {
        let s = s as u32;
        assert_eq!(seg.flags, 0x0040_0000 | (s << 8) | 0x35);
        assert_eq!(seg.v_base_addr, 0xB000_0000 + s);
        assert_eq!(seg.file_offset, 0x100 * (s + 1));
    }
    // the second entry is 116 bytes further
    assert_eq!(m.entry(1).unwrap().module_id, 2);
    assert_eq!(m.entry(1).unwrap().uuid[0], 0x20);
    assert!(m.entry(2).is_none());
}

#[test]
fn module_type_bits_and_segment_flags_decode() {
    let p = modules(1, 0);
    let e = parse_modules_info(&p).unwrap().entry(0).unwrap();
    // type 0xB1 = 1011_0001: load_type 1, auto_start (bit 4) 1, domain_ll (5) 1, domain_dp (6) 0, lib_code (7) 1
    assert_eq!(e.load_type(), 1);
    assert!(e.auto_start());
    assert!(e.domain_ll());
    assert!(!e.domain_dp());
    assert!(e.lib_code());
    // segment flags 0x00400035: contents 1, alloc 0, load 1, readonly 0, code 1, data 1, type 0, length 0x40
    let s = e.segments[0];
    assert_eq!((s.contents(), s.alloc(), s.load(), s.readonly(), s.code(), s.data()), (true, false, true, false, true, true));
    assert_eq!((s.seg_type(), s.length()), (0, 0x40));
    assert_eq!(e.segments[1].seg_type(), 1);
    assert_eq!(e.segments[2].seg_type(), 2);
    let f = SegmentDesc { flags: 0xFFFF_FFFF, v_base_addr: 0, file_offset: 0 };
    assert!(f.alloc() && f.readonly());
    assert_eq!((f.seg_type(), f.length()), (0xF, 0xFFFF));
    // a flags value with only the alloc and readonly bits
    let g = SegmentDesc { flags: 0b1010, v_base_addr: 0, file_offset: 0 };
    assert_eq!((g.contents(), g.alloc(), g.load(), g.readonly(), g.code(), g.data()), (false, true, false, true, false, false));
}

#[test]
fn a_module_is_loaded_when_builtin_or_flagged() {
    // messages.h:594-601
    let mut p = modules(1, 0);
    let set = |p: &mut Vec<u8>, state: u16, ty: u32| {
        p[4 + 2..4 + 4].copy_from_slice(&state.to_le_bytes());
        p[4 + 28..4 + 32].copy_from_slice(&ty.to_le_bytes());
    };
    set(&mut p, 0, AVS_MODULE_LOAD_TYPE_BUILTIN);
    assert!(parse_modules_info(&p).unwrap().entry(0).unwrap().is_loaded());
    set(&mut p, 0, AVS_MODULE_LOAD_TYPE_LOADABLE);
    assert!(!parse_modules_info(&p).unwrap().entry(0).unwrap().is_loaded());
    set(&mut p, AVS_MODULE_STATE_LOADED, AVS_MODULE_LOAD_TYPE_LOADABLE);
    assert!(parse_modules_info(&p).unwrap().entry(0).unwrap().is_loaded());
    set(&mut p, 2, AVS_MODULE_LOAD_TYPE_LOADABLE);
    assert!(!parse_modules_info(&p).unwrap().entry(0).unwrap().is_loaded(), "only bit 0 means loaded");
    assert_eq!((AVS_MODULE_LOAD_TYPE_BUILTIN, AVS_MODULE_LOAD_TYPE_LOADABLE, AVS_MODULE_STATE_LOADED), (0, 1, 1));
}

#[test]
fn modules_info_is_found_by_uuid() {
    let p = modules(3, 0);
    let m = parse_modules_info(&p).unwrap();
    let u2: [u8; 16] = core::array::from_fn(|i| 0x20 + i as u8);
    assert_eq!(m.find_uuid(&u2).unwrap().module_id, 2);
    assert!(m.find_uuid(&[0xEE; 16]).is_none());
}

#[test]
fn modules_info_refusals() {
    assert_eq!(parse_modules_info(&[]), Err(IpcError::EmptyPayload { what: "modules info" }));
    assert_eq!(parse_modules_info(&[1, 0, 0]), Err(IpcError::Truncated { what: "modules info count", need: 4, have: 3 }));
    // the table says 2 entries but carries 1 and a half
    let mut p = modules(2, 0);
    p.truncate(4 + 116 + 58);
    assert_eq!(parse_modules_info(&p), Err(IpcError::Truncated { what: "modules info entries", need: 232, have: 174 }));
    // a count that would overflow the multiplication
    assert!(matches!(parse_modules_info(&dw(u32::MAX)), Err(IpcError::Truncated { .. })));
    // zero entries is a valid, empty table
    assert_eq!(parse_modules_info(&dw(0)).unwrap().count(), 0);
    // trailing bytes are ignored
    assert_eq!(parse_modules_info(&modules(1, 50)).unwrap().count(), 1);
    assert_eq!(MODULE_ENTRY_SIZE, 116);
}

#[test]
fn tlv_header_fields_use_all_four_bytes() {
    // a type and a length with every byte distinct
    let mut p = Vec::new();
    p.extend_from_slice(&dw(0x0D0C_0B0A));
    p.extend_from_slice(&dw(300));
    p.extend(vec![0x5A; 300]);
    let t = tlvs(&p).next().unwrap().unwrap();
    assert_eq!(t.ty, 0x0D0C_0B0A);
    assert_eq!(t.value.len(), 300);
    assert!(tlvs(&p).nth(1).is_none());
    // a length with byte 2 and 3 set is far beyond the payload
    let mut q = Vec::new();
    q.extend_from_slice(&dw(1));
    q.extend_from_slice(&dw(0x0203_0405));
    q.extend(vec![0u8; 16]);
    assert!(matches!(tlvs(&q).next().unwrap(), Err(IpcError::Truncated { what: "tlv value", .. })));
}

#[test]
fn segment_flag_bits_are_independent() {
    let only = |bit: u32| SegmentDesc { flags: 1 << bit, v_base_addr: 0, file_offset: 0 };
    let names = |s: &SegmentDesc| (s.contents(), s.alloc(), s.load(), s.readonly(), s.code(), s.data());
    assert_eq!(names(&only(0)), (true, false, false, false, false, false));
    assert_eq!(names(&only(1)), (false, true, false, false, false, false));
    assert_eq!(names(&only(2)), (false, false, true, false, false, false));
    assert_eq!(names(&only(3)), (false, false, false, true, false, false));
    assert_eq!(names(&only(4)), (false, false, false, false, true, false));
    assert_eq!(names(&only(5)), (false, false, false, false, false, true));
    // reserved bits 6-7, the type nibble 8-11, reserved 12-15 and the length 16-31 are not flags
    for bit in [6u32, 7, 12, 13, 14, 15] {
        assert_eq!(names(&only(bit)), (false, false, false, false, false, false), "bit {bit}");
    }
    assert_eq!(only(8).seg_type(), 1);
    assert_eq!(only(11).seg_type(), 8);
    assert_eq!(only(16).length(), 1);
    assert_eq!(only(31).length(), 0x8000);
}

#[test]
fn module_type_bits_are_independent() {
    let e = |bits: u32| {
        let mut p = modules(1, 0);
        p[4 + 28..4 + 32].copy_from_slice(&bits.to_le_bytes());
        parse_modules_info(Box::leak(p.into_boxed_slice())).unwrap().entry(0).unwrap()
    };
    let flags = |m: &ModuleEntry| (m.auto_start(), m.domain_ll(), m.domain_dp(), m.lib_code());
    assert_eq!(e(0x10).load_type(), 0);
    assert_eq!(flags(&e(0x10)), (true, false, false, false));
    assert_eq!(flags(&e(0x20)), (false, true, false, false));
    assert_eq!(flags(&e(0x40)), (false, false, true, false));
    assert_eq!(flags(&e(0x80)), (false, false, false, true));
    assert_eq!(flags(&e(0x0F)), (false, false, false, false));
    assert_eq!(e(0x0F).load_type(), 0xF);
    assert_eq!(e(0xF0).load_type(), 0);
    assert_eq!(e(0x1).load_type(), 1);
}

#[test]
fn a_tlv_length_wider_than_sixteen_bits_is_read_whole() {
    // length 0x1_0005 = 65541: byte 2 of the length dword is 1
    let len = 0x1_0005usize;
    let mut p = Vec::new();
    p.extend_from_slice(&dw(7));
    p.extend_from_slice(&dw(len as u32));
    p.extend(vec![0xAB; len]);
    let t = tlvs(&p).next().unwrap().unwrap();
    assert_eq!((t.ty, t.value.len()), (7, len));
    // one byte short of what it claims
    p.pop();
    assert_eq!(tlvs(&p).next().unwrap(), Err(IpcError::Truncated { what: "tlv value", need: len, have: len - 1 }));
}
