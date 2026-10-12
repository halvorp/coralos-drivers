// SPDX-License-Identifier: GPL-2.0-only
//! The 64-bit header: every bit-field pinned against the Linux declaration (messages.h:40-235), and every request builder against
//! a header worked out BY HAND from the GCC little-endian bit-field layout (first-declared field in the lowest bits).
use avs_ipc_core::error::IpcError;
use avs_ipc_core::header::*;
use avs_ipc_core::request::*;

#[test]
fn every_field_sits_where_the_declaration_puts_it() {
    // (field, shift, width) - written out from messages.h, one line per declaration
    let table: &[(Field, u32, u32)] = &[
        (MSG_TYPE, 24, 5),
        (MSG_DIRECTION, 29, 1),
        (MSG_TARGET, 30, 1),
        (BOOT_ROM_CTRL_MSG_TYPE, 0, 9),
        (BOOT_DMA_ID, 9, 5),
        (BOOT_PURGE_REQUEST, 14, 1),
        (LOAD_MULTI_MOD_CNT, 0, 8),
        (CREATE_PPL_MEM_SIZE, 0, 11),
        (CREATE_PPL_PRIORITY, 11, 5),
        (CREATE_PPL_INSTANCE_ID, 16, 8),
        (PPL_INSTANCE_ID, 16, 8),
        (SET_PPL_STATE, 0, 16),
        (SET_PPL_ID, 16, 8),
        (GET_PPL_ID, 0, 8),
        (LOAD_LIB_DMA_ID, 0, 5),
        (LOAD_LIB_ID, 16, 4),
        (CREATE_PPL_LP, 32, 1),
        (CREATE_PPL_ATTRIBUTES, 36, 16),
        (MOD_MODULE_ID, 0, 16),
        (MOD_INSTANCE_ID, 16, 8),
        (INIT_PARAM_BLOCK_SIZE, 32, 16),
        (INIT_PPL_INSTANCE_ID, 48, 8),
        (INIT_CORE_ID, 56, 4),
        (INIT_PROC_DOMAIN, 60, 1),
        (LARGE_DATA_OFF_SIZE, 32, 20),
        (LARGE_PARAM_ID, 52, 8),
        (LARGE_FINAL_BLOCK, 60, 1),
        (LARGE_INIT_BLOCK, 61, 1),
        (BIND_DST_MODULE_ID, 32, 16),
        (BIND_DST_INSTANCE_ID, 48, 8),
        (BIND_DST_QUEUE, 56, 3),
        (BIND_SRC_QUEUE, 59, 3),
        (D0IX_WAKE, 32, 1),
        (D0IX_STREAMING, 33, 1),
        (D0IX_PREVENT_PG, 34, 1),
        (D0IX_PREVENT_LOCAL_CG, 35, 1),
        (REPLY_STATUS, 0, 24),
        (REPLY_LOAD_ERR_MOD_ID, 32, 16),
        (REPLY_PPL_STATE, 32, 5),
        (REPLY_LARGE_DATA_OFF_SIZE, 32, 20),
        (REPLY_LARGE_PARAM_ID, 52, 8),
        (REPLY_LARGE_FINAL_BLOCK, 60, 1),
        (REPLY_LARGE_INIT_BLOCK, 61, 1),
        (NOTIFY_TYPE, 16, 8),
        (NOTIFY_LOG_CORE, 12, 4),
        (NOTIFY_COREDUMP_CORE_ID, 32, 2),
        (NOTIFY_COREDUMP_STACK_SIZE, 34, 16),
    ];
    for (f, shift, width) in table {
        assert_eq!((f.shift, f.width), (*shift, *width), "{}", f.name);
        // the field takes its maximum, refuses one more, and touches nothing outside its bits
        let mut h = 0u64;
        f.put(&mut h, f.max()).unwrap();
        assert_eq!(h, ((1u64 << width) - 1) << shift, "{}", f.name);
        assert_eq!(f.get(h), f.max() as u32, "{}", f.name);
        let before = h;
        assert_eq!(f.put(&mut h, f.max() + 1), Err(IpcError::FieldOverflow { field: f.name, value: f.max() + 1, width: *width }));
        assert_eq!(h, before, "a refused put must not change the header");
    }
}

#[test]
fn field_put_replaces_only_its_own_bits() {
    let mut h = u64::MAX;
    LOAD_LIB_ID.put(&mut h, 0).unwrap();
    assert_eq!(h, !(0xFu64 << 16));
    let mut h = 0u64;
    BIND_SRC_QUEUE.put(&mut h, 5).unwrap();
    BIND_DST_QUEUE.put(&mut h, 3).unwrap();
    assert_eq!(h, (5u64 << 59) | (3u64 << 56));
}

#[test]
fn message_type_numbers() {
    let g = [
        (AVS_GLB_ROM_CONTROL, 1),
        (AVS_GLB_LOAD_MULTIPLE_MODULES, 15),
        (AVS_GLB_UNLOAD_MULTIPLE_MODULES, 16),
        (AVS_GLB_CREATE_PIPELINE, 17),
        (AVS_GLB_DELETE_PIPELINE, 18),
        (AVS_GLB_SET_PIPELINE_STATE, 19),
        (AVS_GLB_GET_PIPELINE_STATE, 20),
        (AVS_GLB_LOAD_LIBRARY, 24),
        (AVS_GLB_NOTIFICATION, 27),
    ];
    for (c, v) in g {
        assert_eq!(c, v);
    }
    let m = [
        (AVS_MOD_INIT_INSTANCE, 0),
        (AVS_MOD_LARGE_CONFIG_GET, 3),
        (AVS_MOD_LARGE_CONFIG_SET, 4),
        (AVS_MOD_BIND, 5),
        (AVS_MOD_UNBIND, 6),
        (AVS_MOD_SET_DX, 7),
        (AVS_MOD_SET_D0IX, 8),
        (AVS_MOD_DELETE_INSTANCE, 11),
    ];
    for (c, v) in m {
        assert_eq!(c, v);
    }
    assert_eq!((AVS_FW_GEN_MSG, AVS_MOD_MSG, AVS_MSG_REQUEST, AVS_MSG_REPLY), (0, 1, 0, 1));
    assert_eq!(AVS_ROM_SET_BOOT_CONFIG, 0);
    assert_eq!(
        (AVS_PPL_STATE_INVALID, AVS_PPL_STATE_UNINITIALIZED, AVS_PPL_STATE_RESET, AVS_PPL_STATE_PAUSED, AVS_PPL_STATE_RUNNING),
        (0, 1, 2, 3, 4)
    );
    // the message types are pairwise distinct inside each numbering
    for (i, (_, a)) in g.iter().enumerate() {
        for (_, b) in g.iter().skip(i + 1) {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn request_header_constructors() {
    // AVS_GLOBAL_REQUEST / AVS_MODULE_REQUEST / AVS_NOTIFICATION (messages.h:239-259)
    assert_eq!(global_request(AVS_GLB_LOAD_LIBRARY).unwrap(), 0x1800_0000);
    assert_eq!(module_request(AVS_MOD_BIND).unwrap(), 0x4500_0000);
    assert_eq!(notification(8).unwrap(), 0x3B08_0000);
    assert_eq!(primary(0x1234_5678_9ABC_DEF0), 0x9ABC_DEF0);
    assert_eq!(ext(0x1234_5678_9ABC_DEF0), 0x1234_5678);
}

#[test]
fn is_reply_is_direction_reply_and_not_a_notification() {
    assert!(is_reply(0x2000_0000)); // direction REPLY, type 0
    assert!(is_reply(0x6300_0000)); // module reply
    assert!(!is_reply(0x0000_0000)); // a request
    assert!(!is_reply(0x1800_0000)); // a LOAD_LIBRARY request
    assert!(!is_reply(0x3B08_0000)); // direction REPLY but type 27: a notification
    assert!(is_reply(0x3A00_0000)); // REPLY, type 26: not 27, so a reply
}

fn hdr(r: Request<'_>) -> (u32, u32) {
    (primary(r.header), ext(r.header))
}

#[test]
fn set_boot_config_header() {
    // ROM_CONTROL (1) << 24 | dma_id 5 << 9 | purge << 14
    let r = set_boot_config(5, true).unwrap();
    assert_eq!(hdr(r), (0x0100_4A00, 0));
    assert_eq!((r.flow, r.timeout_ms, r.name), (Flow::Rom, 300, "set boot config"));
    assert_eq!(hdr(set_boot_config(0, false).unwrap()), (0x0100_0000, 0));
    assert_eq!(hdr(set_boot_config(31, false).unwrap()), (0x0100_3E00, 0));
    assert_eq!(set_boot_config(32, false), Err(IpcError::FieldOverflow { field: "boot_cfg.dma_id", value: 32, width: 5 }));
}

#[test]
fn load_and_unload_modules_headers_and_payload() {
    let r = load_module(0x1D).unwrap();
    assert_eq!(hdr(r), (0x0F00_0001, 0));
    assert_eq!(r.payload.as_bytes(), &[0x1D, 0x00]);
    assert_eq!((r.timeout_ms, r.flow), (5000, Flow::Msg)); // AVS_CL_TIMEOUT_MS (messages.c:13)
    let mut buf = [0u8; 16];
    let r = load_modules(&[0x1234, 0xABCD, 7], &mut buf).unwrap();
    assert_eq!(hdr(r), (0x0F00_0003, 0));
    assert_eq!(r.payload.as_bytes(), &[0x34, 0x12, 0xCD, 0xAB, 0x07, 0x00]);
    assert_eq!(r.timeout_ms, 5000);
    let mut buf = [0u8; 16];
    let r = unload_modules(&[9, 8], &mut buf).unwrap();
    assert_eq!(hdr(r), (0x1000_0002, 0));
    assert_eq!(r.payload.as_bytes(), &[9, 0, 8, 0]);
    assert_eq!(r.timeout_ms, 300); // unload uses the default budget (messages.c:42)
    // 256 modules do not fit the 8-bit count
    let ids = [0u16; 256];
    let mut big = [0u8; 512];
    assert_eq!(load_modules(&ids, &mut big), Err(IpcError::FieldOverflow { field: "load_multi_mods.mod_cnt", value: 256, width: 8 }));
    // and a buffer too small for the ids is refused, not overrun
    let mut small = [0u8; 3];
    assert!(matches!(load_modules(&[1, 2], &mut small), Err(IpcError::PayloadTooBig { len: 4, max: 3 })));
}

#[test]
fn load_library_header() {
    // type 24 << 24, lib_id << 16, dma_id in bits 0..4
    let r = load_library(0, 2).unwrap();
    assert_eq!(hdr(r), (0x1802_0000, 0));
    assert_eq!((r.timeout_ms, r.name), (5000, "load library"));
    assert_eq!(hdr(load_library(3, 15).unwrap()), (0x180F_0003, 0));
    assert_eq!(load_library(0, 16), Err(IpcError::FieldOverflow { field: "load_lib.lib_id", value: 16, width: 4 }));
    assert_eq!(load_library(32, 0), Err(IpcError::FieldOverflow { field: "load_lib.dma_id", value: 32, width: 5 }));
}

#[test]
fn pipeline_headers() {
    // create: type 17, mem size 0x123 (bits 0..10), priority 3 (11..15), instance 5 (16..23); ext lp 1, attributes 0xABC << 4
    let r = create_pipeline(0x123, 3, 5, true, 0xABC).unwrap();
    assert_eq!(hdr(r), (0x1105_1923, 0x000A_BC1));
    assert_eq!(hdr(create_pipeline(0, 0, 0, false, 0).unwrap()), (0x1100_0000, 0));
    assert_eq!(create_pipeline(0x800, 0, 0, false, 0), Err(IpcError::FieldOverflow { field: "create_ppl.ppl_mem_size", value: 0x800, width: 11 }));
    assert_eq!(create_pipeline(0, 32, 0, false, 0), Err(IpcError::FieldOverflow { field: "create_ppl.ppl_priority", value: 32, width: 5 }));
    assert_eq!(hdr(delete_pipeline(7).unwrap()), (0x1207_0000, 0));
    // set state: state in 0..15, ppl_id in 16..23
    assert_eq!(hdr(set_pipeline_state(3, AVS_PPL_STATE_RUNNING).unwrap()), (0x1303_0004, 0));
    assert_eq!(hdr(set_pipeline_state(3, AVS_PPL_STATE_PAUSED).unwrap()), (0x1303_0003, 0));
    // get state: ppl_id in bits 0..7 (NOT 16..23 like the others)
    assert_eq!(hdr(get_pipeline_state(9).unwrap()), (0x1400_0009, 0));
}

#[test]
fn init_instance_header_counts_the_parameter_block_in_dwords() {
    let p10 = [0u8; 10];
    let r = init_instance(0x1234, 7, 3, 1, 1, &p10).unwrap();
    assert_eq!(hdr(r), (0x4007_1234, 0x1103_0003)); // 10 bytes -> 3 dwords
    assert_eq!(r.payload.as_bytes().len(), 10);
    let r = init_instance(1, 0, 0, 0, 0, &[0u8; 8]).unwrap();
    assert_eq!(ext(r.header), 2);
    let r = init_instance(1, 0, 0, 0, 0, &[]).unwrap();
    assert_eq!(ext(r.header), 0);
    let r = init_instance(1, 0, 0, 0, 0, &[0u8; 1]).unwrap();
    assert_eq!(ext(r.header), 1);
    assert_eq!(init_instance(1, 0, 0, 16, 0, &[]), Err(IpcError::FieldOverflow { field: "ext.init_instance.core_id", value: 16, width: 4 }));
    assert_eq!(init_instance(1, 0, 0, 0, 2, &[]), Err(IpcError::FieldOverflow { field: "ext.init_instance.proc_domain", value: 2, width: 1 }));
    let big = vec![0u8; 4097];
    assert_eq!(init_instance(1, 0, 0, 0, 0, &big), Err(IpcError::PayloadTooBig { len: 4097, max: 4096 }));
    assert!(init_instance(1, 0, 0, 0, 0, &big[..4096]).is_ok());
}

#[test]
fn instance_and_bind_headers() {
    assert_eq!(hdr(delete_instance(0x1234, 7).unwrap()), (0x4B07_1234, 0));
    // bind: type 5; ext dst module 0x20, dst instance 3 (16..23), dst queue 5 (24..26), src queue 6 (27..29)
    assert_eq!(hdr(bind(0x10, 2, 0x20, 3, 5, 6).unwrap()), (0x4502_0010, 0x3503_0020));
    assert_eq!(hdr(unbind(0x10, 2, 0x20, 3, 5, 6).unwrap()), (0x4602_0010, 0x3503_0020));
    assert_eq!(bind(0, 0, 0, 0, 8, 0), Err(IpcError::FieldOverflow { field: "ext.bind_unbind.dst_queue", value: 8, width: 3 }));
    assert_eq!(bind(0, 0, 0, 0, 0, 8), Err(IpcError::FieldOverflow { field: "ext.bind_unbind.src_queue", value: 8, width: 3 }));
}

#[test]
fn power_headers() {
    let r = set_dx(1, true).unwrap();
    assert_eq!(hdr(r), (0x4700_0000, 0));
    assert_eq!(r.payload.as_bytes(), &[1, 0, 0, 0, 1, 0, 0, 0]); // struct avs_dxstate_info { core_mask, dx_mask }
    let r = set_dx(0b101, false).unwrap();
    assert_eq!(r.payload.as_bytes(), &[5, 0, 0, 0, 0, 0, 0, 0]);
    let r = set_dx(0b101, true).unwrap();
    assert_eq!(r.payload.as_bytes(), &[5, 0, 0, 0, 5, 0, 0, 0]);
    // set_d0ix: wake = enable_pg, streaming, prevent_pg = !enable_pg
    assert_eq!(hdr(set_d0ix(true, false).unwrap()), (0x4800_0000, 0b001));
    assert_eq!(hdr(set_d0ix(false, true).unwrap()), (0x4800_0000, 0b110));
    assert_eq!(hdr(set_d0ix(true, true).unwrap()), (0x4800_0000, 0b011));
    assert_eq!(hdr(set_d0ix(false, false).unwrap()), (0x4800_0000, 0b100));
}

#[test]
fn wire_header_sets_the_request_busy_bit_in_the_primary_dword() {
    let r = load_library(0, 2).unwrap();
    assert_eq!(r.wire_header(), 0x0000_0000_9802_0000);
    let r = bind(0x10, 2, 0x20, 3, 5, 6).unwrap();
    assert_eq!(r.wire_header(), 0x3503_0020_C502_0010);
}

#[test]
fn large_config_set_is_one_block_up_to_the_mailbox_size() {
    // module 0x20, instance 1, param 9; 10 bytes: init+final, data_off_size = total
    let blocks: Vec<_> = LargeSet::new(0x20, 1, 9, 10).unwrap().map(|b| b.unwrap()).collect();
    assert_eq!(blocks.len(), 1);
    assert_eq!((primary(blocks[0].header), ext(blocks[0].header)), (0x4401_0020, 0x3090_000A));
    assert_eq!((blocks[0].offset, blocks[0].len), (0, 10));
    // exactly the mailbox: still one block
    let blocks: Vec<_> = LargeSet::new(0x20, 1, 9, 4096).unwrap().map(|b| b.unwrap()).collect();
    assert_eq!(blocks.len(), 1);
    assert_eq!(ext(blocks[0].header), 0x3000_0000 | 0x90_0000 | 4096);
    // empty: one empty block
    let blocks: Vec<_> = LargeSet::new(0x20, 1, 9, 0).unwrap().map(|b| b.unwrap()).collect();
    assert_eq!(blocks.len(), 1);
    assert_eq!((blocks[0].offset, blocks[0].len, ext(blocks[0].header)), (0, 0, 0x3090_0000));
}

#[test]
fn large_config_set_splits_at_the_mailbox_and_states_total_then_offsets() {
    // messages.c:271-306
    let blocks: Vec<_> = LargeSet::new(0x20, 1, 9, 10_000).unwrap().map(|b| b.unwrap()).collect();
    assert_eq!(blocks.len(), 3);
    // first: init, not final, data_off_size = TOTAL
    assert_eq!((blocks[0].offset, blocks[0].len, ext(blocks[0].header)), (0, 4096, 0x2000_0000 | 0x90_0000 | 10_000));
    // second: neither, data_off_size = OFFSET
    assert_eq!((blocks[1].offset, blocks[1].len, ext(blocks[1].header)), (4096, 4096, 0x90_0000 | 4096));
    // third: final, offset 8192, 1808 bytes
    assert_eq!((blocks[2].offset, blocks[2].len, ext(blocks[2].header)), (8192, 1808, 0x1000_0000 | 0x90_0000 | 8192));
    for b in &blocks {
        assert_eq!(primary(b.header), 0x4401_0020);
    }
    assert_eq!(blocks.iter().map(|b| b.len).sum::<usize>(), 10_000);
    // 8192 exactly is two blocks, the second final
    let blocks: Vec<_> = LargeSet::new(0, 0, 0, 8192).unwrap().map(|b| b.unwrap()).collect();
    assert_eq!(blocks.len(), 2);
    assert_eq!(LARGE_FINAL_BLOCK.get(blocks[1].header), 1);
    assert_eq!(LARGE_FINAL_BLOCK.get(blocks[0].header), 0);
}

#[test]
fn large_config_set_refuses_a_total_the_size_field_cannot_hold() {
    assert!(LargeSet::new(0, 0, 0, 0xF_FFFF).is_ok());
    assert_eq!(LargeSet::new(0, 0, 0, 0x10_0000).unwrap_err(), IpcError::LargeConfigTooBig { total: 0x10_0000 });
}

#[test]
fn large_config_get_headers() {
    // type 3; ext: data_off_size = request length, param 7 << 20, init_block (bit 29), final_block clear
    let r = get_large_config(0, 0, 7, &[]).unwrap();
    assert_eq!(hdr(r), (0x4300_0000, 0x2070_0000));
    let req = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let r = get_large_config(0x20, 1, 7, &req).unwrap();
    assert_eq!(hdr(r), (0x4301_0020, 0x2070_0008));
    assert_eq!(r.payload.as_bytes(), &req);
    assert_eq!(hdr(get_fw_config().unwrap()), (0x4300_0000, 0x2070_0000));
    assert_eq!(hdr(get_hw_config().unwrap()), (0x4300_0000, 0x2080_0000));
    assert_eq!(hdr(get_modules_info().unwrap()), (0x4300_0000, 0x2090_0000));
    assert_eq!(AVS_BASEFW_FIRMWARE_CONFIG, 7);
    assert_eq!(AVS_BASEFW_HARDWARE_CONFIG, 8);
    assert_eq!(AVS_BASEFW_MODULES_INFO, 9);
    assert_eq!((AVS_BASEFW_ENABLE_LOGS, AVS_BASEFW_LIBRARIES_INFO, AVS_BASEFW_SYSTEM_TIME), (6, 16, 20));
    assert_eq!((AVS_BASEFW_MOD_ID, AVS_BASEFW_INST_ID), (0, 0));
}

#[test]
fn system_time_is_low_dword_then_high_dword() {
    assert_eq!(system_time_payload(0x1122_3344_5566_7788), [0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11]);
}

#[test]
fn a_payload_larger_than_the_mailbox_is_refused_everywhere_it_can_arise() {
    let big = vec![0u8; 4097];
    assert!(matches!(get_large_config(0, 0, 0, &big), Err(IpcError::PayloadTooBig { len: 4097, max: 4096 })));
    let ids = vec![0u16; 2049];
    let mut buf = vec![0u8; 5000];
    // the 8-bit module count is hit long before the mailbox could be
    assert!(matches!(unload_modules(&ids, &mut buf), Err(IpcError::FieldOverflow { field: "load_multi_mods.mod_cnt", value: 2049, width: 8 })));
}

#[test]
fn a_buffer_exactly_the_size_of_the_module_ids_is_enough() {
    let mut exact = [0u8; 4];
    let r = load_modules(&[1, 2], &mut exact).unwrap();
    assert_eq!(r.payload.as_bytes(), &[1, 0, 2, 0]);
    let mut none = [0u8; 0];
    assert!(load_modules(&[], &mut none).is_ok());
    let mut one_short = [0u8; 3];
    assert!(load_modules(&[1, 2], &mut one_short).is_err());
}

#[test]
fn header_constructors_refuse_a_type_wider_than_the_field() {
    assert_eq!(global_request(32), Err(IpcError::FieldOverflow { field: "msg_type", value: 32, width: 5 }));
    assert_eq!(module_request(32), Err(IpcError::FieldOverflow { field: "msg_type", value: 32, width: 5 }));
    assert_eq!(notification(256), Err(IpcError::FieldOverflow { field: "notify_msg_type", value: 256, width: 8 }));
    assert!(global_request(31).is_ok());
    assert!(notification(255).is_ok());
}
