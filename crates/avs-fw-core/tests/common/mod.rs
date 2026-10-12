// SPDX-License-Identifier: GPL-2.0-only
//! A scripted device for driving the planners: runs one to its end, logging every step it asked for.
#![allow(dead_code)]
use avs_fw_core::effect::*;

/// The answer of a device where everything works.
pub fn ok_answer(s: &Step) -> Input {
    match s.op {
        Op::Update { .. } | Op::Write { .. } | Op::DelayUs(_) | Op::CopyToDma { .. } | Op::IpcSend(_) => Input::Done,
        Op::Read { .. } => Input::Value(0),
        Op::Poll(p) => Input::Value(p.want),
        Op::Wait { event: Event::CldmaIoc, .. } => Input::IocStatus(0x04),
        Op::Wait { .. } => Input::EventFired,
        Op::IpcAwait { .. } => Input::IpcStatus(0),
    }
}

/// Drive `next` to `Finished`/`Failed`, never more than `cap` steps (a planner that does not stop fails the test, not the box).
pub fn run(mut next: impl FnMut(Input) -> Output, mut answer: impl FnMut(&Step) -> Input, cap: usize) -> (Vec<Step>, Output) {
    let mut log = Vec::new();
    let mut out = next(Input::Begin);
    loop {
        match out {
            Output::Do(s) => {
                log.push(s);
                assert!(log.len() <= cap, "planner did not stop within {cap} steps; last: {}", s.label);
                let a = answer(&s);
                out = next(a);
            }
            end => return (log, end),
        }
    }
}

pub fn labels(log: &[Step]) -> Vec<&'static str> {
    log.iter().map(|s| s.label).collect()
}

/// A synthetic image: optional `$AE1` extended manifest of `ext_len` bytes (16-byte header + zero padding), then the Skylake
/// magic at the start of the body, the manifest at body offset 0x284, then `tail` zero bytes.
pub fn skl_image(ext_len: Option<u32>, name: &[u8; 8], v: (u16, u16, u16, u16), tail: usize) -> Vec<u8> {
    let mut img = Vec::new();
    if let Some(l) = ext_len {
        img.extend_from_slice(&0x31454124u32.to_le_bytes());
        img.extend_from_slice(&l.to_le_bytes());
        img.extend_from_slice(&1u16.to_le_bytes());
        img.extend_from_slice(&2u16.to_le_bytes());
        img.extend_from_slice(&3u32.to_le_bytes());
        img.resize(l as usize, 0);
    }
    let base = img.len();
    img.extend_from_slice(&6u32.to_le_bytes());
    img.resize(base + 0x284, 0);
    img.extend_from_slice(&0x314d4124u32.to_le_bytes()); // id
    img.extend_from_slice(&0x34u32.to_le_bytes()); // len
    img.extend_from_slice(name);
    img.extend_from_slice(&0x3bu32.to_le_bytes()); // preload_page_count
    img.extend_from_slice(&0x5u32.to_le_bytes()); // img_flags
    img.extend_from_slice(&0x1fu32.to_le_bytes()); // feature_mask
    for x in [v.0, v.1, v.2, v.3] {
        img.extend_from_slice(&x.to_le_bytes());
    }
    img.resize(img.len() + tail, 0);
    img
}
