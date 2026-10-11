// SPDX-License-Identifier: GPL-2.0-only
//! Draining the controller and posting the frames — everything the main loop does between two waits, with the hardware and
//! the endpoint behind traits so a host test drives it end to end.
use crate::init::Device;
use crate::log::{self, Log};
use coralos_rt::keywire::{encode, ALL_UP, PRESS};
use i8042_core::controller::Source;
use i8042_core::Port;

/// Where a frame goes: the keyboard endpoint in the device, a vector in a test. False = the frame was NOT delivered.
pub trait Sink {
    fn send(&mut self, frame: [u8; 3]) -> bool;
}

/// Bytes read per wake at most: a controller that never clears its output-buffer flag cannot hold the task.
pub const MAX_PER_WAKE: usize = 32;

/// What the wire has lost, and what has been said about it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Outbox {
    /// A frame was refused: the reader's key state may be stale until a resynchronisation gets through.
    pub lost: bool,
    pub sent: u32,
    pub dropped: u32,
    pub resyncs: u32,
    pub first_key_said: bool,
    pub aux_bytes: u32,
}

impl Outbox {
    pub const fn new() -> Self {
        Outbox { lost: false, sent: 0, dropped: 0, resyncs: 0, first_key_said: false, aux_bytes: 0 }
    }
}

fn post<S: Sink>(sink: &mut S, out: &mut Outbox, ev: (u8, u8), held: &dyn Fn(&mut dyn FnMut(u8))) {
    if out.lost {
        // The reader may have missed a release (or a press). Tell it nothing is held, then what is: the CURRENT state,
        // which already includes this event, so the event itself is not sent again.
        let mut ok = sink.send(encode(0, ALL_UP));
        held(&mut |u| ok = ok && sink.send(encode(u, PRESS)));
        if ok {
            out.lost = false;
            out.resyncs += 1;
            out.sent += 1;
        } else {
            out.dropped += 1;
        }
        return;
    }
    if sink.send(encode(ev.0, ev.1)) {
        out.sent += 1;
    } else {
        out.lost = true;
        out.dropped += 1;
    }
}

/// Read what the controller holds (bounded) and post the key events it makes. Returns the bytes read.
pub fn drain<P: Port, S: Sink, L: Log>(dev: &mut Device<P>, out: &mut Outbox, sink: &mut S, lg: &mut L) -> usize {
    let mut n = 0;
    while n < MAX_PER_WAKE {
        let Some((byte, src)) = dev.ctl.read_byte() else { break };
        n += 1;
        if src == Source::Aux {
            out.aux_bytes += 1; // the AUX port is disabled at bring-up; a byte from it is a stray, and is dropped
            continue;
        }
        let mut evs: [(u8, u8); 2] = [(0, 0); 2];
        let mut k = 0;
        dev.pipe.feed(byte, &mut |e| {
            if k < evs.len() {
                evs[k] = e;
                k += 1;
            }
        });
        for &ev in &evs[..k] {
            if !out.first_key_said {
                out.first_key_said = true;
                log::emit(lg, format_args!("[I8042-DEV FIRST-KEY usage={:#04x} state={}]", ev.0, ev.1));
            }
            let pipe = &dev.pipe;
            post(sink, out, ev, &|f| pipe.for_each_down(f));
        }
    }
    n
}
