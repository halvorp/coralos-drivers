// SPDX-License-Identifier: GPL-2.0-only
#![allow(dead_code)]
use avs_ipc_core::txn::*;

pub fn tlv(ty: u32, value: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&ty.to_le_bytes());
    v.extend_from_slice(&(value.len() as u32).to_le_bytes());
    v.extend_from_slice(value);
    v
}

pub fn dw(x: u32) -> [u8; 4] {
    x.to_le_bytes()
}

/// A reply header: `status` in 0..24, `msg_type` in 24..29, direction REPLY (bit 29), `target` in bit 30; ext in the high dword.
pub fn reply(status: u32, msg_type: u32, target: u32, ext: u32) -> u64 {
    ((ext as u64) << 32) | (status as u64) | ((msg_type as u64) << 24) | (1u64 << 29) | ((target as u64) << 30)
}

/// A notification header: type in 16..24, global type 27, direction REPLY, target 0.
pub fn notify(ty: u32, ext: u32, low16: u32) -> u64 {
    ((ext as u64) << 32) | (low16 as u64) | ((ty as u64) << 16) | (27u64 << 24) | (1u64 << 29)
}

/// Drive the transaction with the given answers for waits, returning the ops it asked for.
pub fn drive(ipc: &mut Ipc, first: Output, mut answer: impl FnMut(&Output) -> Input, cap: usize) -> (Vec<Output>, Result<Completed, avs_ipc_core::error::IpcError>) {
    let mut log = vec![first];
    let mut out = first;
    loop {
        match out {
            Output::Done(r) => return (log, r),
            o => {
                assert!(log.len() <= cap, "transaction did not finish in {cap} ops: {o:?}");
                let a = answer(&o);
                out = ipc.step(a);
                log.push(out);
            }
        }
    }
}

/// An Ipc that has seen FW_READY.
pub fn ready_ipc(c: Correlation) -> Ipc {
    let mut ipc = Ipc::new(c);
    ipc.on_response(notify(8, 0, 0), 0).unwrap();
    assert!(ipc.ready());
    ipc
}
