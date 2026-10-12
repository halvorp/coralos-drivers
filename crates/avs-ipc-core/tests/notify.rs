// SPDX-License-Identifier: GPL-2.0-only
//! Reply and notification decoding (messages.h:166-235; ipc.c:185-285).
mod common;
use avs_ipc_core::error::IpcError;
use avs_ipc_core::header::*;
use avs_ipc_core::notify::*;
use common::{notify, reply};

#[test]
fn a_reply_decodes_status_type_and_target() {
    let r = Reply::new(0x33123456); // status 0x123456, type 19, REPLY
    assert_eq!(r.status(), 0x123456);
    assert_eq!(r.msg_type(), 19);
    assert_eq!(r.target(), 0);
    assert!(!r.is_not_supported());
    let r = Reply::new(reply(15, 3, 1, 0));
    assert_eq!((r.status(), r.msg_type(), r.target()), (15, 3, 1));
    assert!(r.is_not_supported());
    assert!(!Reply::new(reply(14, 0, 0, 0)).is_not_supported());
    assert!(!Reply::new(reply(16, 0, 0, 0)).is_not_supported());
}

#[test]
fn reply_extension_fields() {
    assert_eq!(Reply::new(reply(0, 20, 0, 4)).ppl_state(), 4);
    assert_eq!(Reply::new(reply(0, 20, 0, 0xFFFF_FFE0)).ppl_state(), 0, "state is five bits wide");
    assert_eq!(Reply::new(reply(7, 15, 0, 0x1234)).err_mod_id(), 0x1234);
    let r = Reply::new(reply(0, 3, 1, 0x1000_0000 | 0x90_0000 | 0x123));
    assert_eq!((r.large_data_off_size(), r.large_final_block(), r.large_init_block()), (0x123, true, false));
    let r = Reply::new(reply(0, 3, 1, 0x2000_0000));
    assert_eq!((r.large_final_block(), r.large_init_block()), (false, true));
}

#[test]
fn reply_payload_length_follows_avs_dsp_receive_rx() {
    // ipc.c:185-207: a failed request carries no payload; LARGE_CONFIG_GET states its own size, capped at the mailbox
    assert_eq!(Reply::new(reply(5, 3, 1, 100)).payload_len(4096), 0);
    assert_eq!(Reply::new(reply(0, 3, 1, 100)).payload_len(4096), 100);
    assert_eq!(Reply::new(reply(0, 3, 1, 4095)).payload_len(4096), 4095);
    assert_eq!(Reply::new(reply(0, 3, 1, 4096)).payload_len(4096), 4096);
    assert_eq!(Reply::new(reply(0, 3, 1, 5000)).payload_len(4096), 4096);
    assert_eq!(Reply::new(reply(0, 3, 1, 0)).payload_len(4096), 0);
    // any other reply: the size the sender reserved
    assert_eq!(Reply::new(reply(0, 20, 0, 4)).payload_len(0), 0);
    assert_eq!(Reply::new(reply(0, 20, 0, 4)).payload_len(8), 8);
    // type 3 on the GLOBAL side is not LARGE_CONFIG_GET
    assert_eq!(Reply::new(reply(0, 3, 0, 100)).payload_len(8), 8);
    // type 4 (LARGE_CONFIG_SET) on the module side is not either
    assert_eq!(Reply::new(reply(0, 4, 1, 100)).payload_len(8), 8);
}

#[test]
fn notification_types() {
    assert_eq!(AVS_NOTIFY_PHRASE_DETECTED, 4);
    assert_eq!(AVS_NOTIFY_RESOURCE_EVENT, 5);
    assert_eq!(AVS_NOTIFY_LOG_BUFFER_STATUS, 6);
    assert_eq!(AVS_NOTIFY_FW_READY, 8);
    assert_eq!(AVS_NOTIFY_EXCEPTION_CAUGHT, 10);
    assert_eq!(AVS_NOTIFY_MODULE_EVENT, 12);
    let kinds = [
        (4, NotifyKind::PhraseDetected),
        (5, NotifyKind::ResourceEvent),
        (6, NotifyKind::LogBufferStatus),
        (8, NotifyKind::FwReady),
        (10, NotifyKind::ExceptionCaught),
        (12, NotifyKind::ModuleEvent),
        (0, NotifyKind::Unknown(0)),
        (7, NotifyKind::Unknown(7)),
        (255, NotifyKind::Unknown(255)),
    ];
    for (t, k) in kinds {
        assert_eq!(Notify::new(notify(t, 0, 0)).kind(), k, "type {t}");
        assert_eq!(Notify::new(notify(t, 0, 0)).type_raw(), t);
    }
}

#[test]
fn notification_fields() {
    let n = Notify::new(notify(6, 0, 0x3000)); // log core in bits 12..15 of the primary
    assert_eq!(n.log_core(), 3);
    let n = Notify::new(notify(10, (0x1234 << 2) | 2, 0));
    assert_eq!((n.coredump_core_id(), n.coredump_stack_size()), (2, 0x1234));
}

#[test]
fn notification_payload_sizes() {
    let size = |t, d| Notify::new(notify(t, 0, 0)).payload_size(d);
    assert_eq!(size(8, 999), Ok(0)); // FW_READY: none
    assert_eq!(size(4, 0), Ok(4)); // sizeof(struct avs_notify_voice_data)
    assert_eq!(size(5, 0), Ok(40)); // sizeof(struct avs_notify_res_data)
    assert_eq!(size(6, 0), Ok(0));
    assert_eq!(size(10, 0), Ok(0));
    assert_eq!(size(12, 0), Ok(12)); // sizeof(struct avs_notify_mod_data) + data_size
    assert_eq!(size(12, 100), Ok(112));
    assert_eq!(size(99, 5), Ok(0)); // unknown
    // beyond the mailbox is refused, exactly at it is not
    assert_eq!(size(12, 4084), Ok(4096));
    assert_eq!(size(12, 4085), Err(IpcError::NotifyPayloadTooBig { size: 4097, max: 4096 }));
}

#[test]
fn payload_size_constants() {
    assert_eq!(NOTIFY_VOICE_DATA_SIZE, 4);
    assert_eq!(NOTIFY_RES_DATA_SIZE, 40);
    assert_eq!(NOTIFY_MOD_DATA_SIZE, 12);
    assert_eq!(AVS_GLB_NOTIFICATION, 27);
}
