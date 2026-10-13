// SPDX-License-Identifier: GPL-2.0-only
//! The connection layer on its own (no uAPI service): a connection's inbound reassembly and outbound queue over a fake kernel, and the
//! PARKED service that answers over the same connections - the legacy `service()` path reassembles exactly as the live loop does.
mod net_common;
use i915_device::conn::{ConnId, ConnTable, FlushCount, Parked, Pulled, Queued, Wake};
use i915_device::log::Log;
use i915_wire::frag::{FragError, Joiner, Limits};
use i915_wire::{Opcode, Status, HEADER_BYTES};
use net_common::*;

#[derive(Default)]
struct Lines(Vec<String>);
impl Log for Lines {
    fn line(&mut self, s: &str) {
        self.0.push(s.trim_end().to_string());
    }
}

fn hello(rid: u64, total: usize) -> Vec<u8> {
    let mut f = frame(Opcode::Hello, rid, 0, &vec![0xA5u8; total - HEADER_BYTES]);
    for (i, b) in f[HEADER_BYTES..].iter_mut().enumerate() {
        *b = (i % 253) as u8;
    }
    f
}

fn table_with(t: &mut FakeT, n: usize) -> (ConnTable, Vec<ConnId>) {
    let mut tab = ConnTable::new();
    let mut ids = vec![];
    for _ in 0..n {
        let id = t.connect();
        assert!(tab.reserve_one());
        assert!(tab.admit(id, Joiner::new(Limits::KERNEL)));
        ids.push(id);
    }
    (tab, ids)
}

#[test]
fn a_table_row_is_reserved_before_it_is_admitted() {
    let mut tab = ConnTable::new();
    assert!(!tab.admit(ConnId(1), Joiner::new(Limits::KERNEL)), "no row was reserved: admission refuses instead of growing blindly");
    assert!(tab.reserve_one());
    assert!(tab.admit(ConnId(1), Joiner::new(Limits::KERNEL)));
    // without a further reservation the table refuses once its reserved room is used, instead of growing where a failure cannot be named
    let mut n = 1u64;
    while tab.admit(ConnId(n + 1), Joiner::new(Limits::KERNEL)) {
        n += 1;
        assert!(n < 1024, "admission never refused");
    }
    assert_eq!(tab.len() as u64, n);
    // the table has no fixed size: it grows as long as the heap does
    for i in 2000..2500u64 {
        assert!(tab.reserve_one() && tab.admit(ConnId(i), Joiner::new(Limits::KERNEL)));
    }
    assert_eq!(tab.len() as u64, n + 500);
}

#[test]
fn a_fifteen_message_reply_into_a_busy_ring_is_kept_and_resumes_in_order() {
    let mut t = FakeT::default();
    t.0.borrow_mut().depth = 16; // the kernel's ring
    let (mut tab, ids) = table_with(&mut t, 1);
    let id = ids[0];
    // the client has 3 replies it has not read yet: 13 slots are free, the frame needs 15
    t.0.borrow_mut().clients.get_mut(&id).unwrap().unread = 3;
    let big = hello(1, 61440);
    let small = hello(2, 100);
    let c = tab.get_mut(id).unwrap();
    assert_eq!(c.send_frame(&mut t, &big), Queued::Kept);
    assert_eq!(t.messages_sent(id), 13, "13 messages fit, then the ring was full: nothing was dropped or truncated");
    assert_eq!(c.send_frame(&mut t, &small), Queued::Kept, "a later frame waits behind the unfinished one (order)");
    assert_eq!(t.messages_sent(id), 13);
    let mut count = FlushCount::default();
    assert_eq!(c.flush(&mut t, &mut count), i915_device::conn::Flush::Pending, "still full");
    assert_eq!(t.messages_sent(id), 13);
    // the client drains its ring; the rest of the big frame and then the small one follow
    t.0.borrow_mut().clients.get_mut(&id).unwrap().unread = 0;
    assert_eq!(c.flush(&mut t, &mut count), i915_device::conn::Flush::Empty);
    assert_eq!(count, FlushCount { sent: 2, dropped: 0 });
    assert_eq!(t.messages_sent(id), 13 + 2 + 1);
    let frames = t.drain_frames(id);
    assert_eq!(frames, vec![big, small], "both frames arrived whole, in the order they were sent");
}

#[test]
fn a_reply_that_cannot_travel_as_messages_is_refused_by_name_and_sends_nothing() {
    let mut t = FakeT::default();
    let (mut tab, ids) = table_with(&mut t, 1);
    let c = tab.get_mut(ids[0]).unwrap();
    assert_eq!(c.send_frame(&mut t, &hello(1, 61441)), Queued::Dropped("FRAG-TOO-MANY-MESSAGES"));
    assert_eq!(c.send_frame(&mut t, &[1, 2, 3]), Queued::Dropped("WIRE-SHORT-BUFFER"), "not a frame");
    assert_eq!(t.messages_sent(ids[0]), 0);
    assert_eq!(c.send_message(&mut t, &[0u8; 4097]), Queued::Dropped("MESSAGE-TOO-LONG"));
    t.0.borrow_mut().clients.get_mut(&ids[0]).unwrap().send_dead = true;
    assert_eq!(c.send_frame(&mut t, &hello(2, 100)), Queued::Dead);
}

#[test]
fn a_frame_the_heap_cannot_hold_is_discarded_whole_not_misread_as_new_frames() {
    let mut t = FakeT::default();
    let (mut tab, ids) = table_with(&mut t, 1);
    let id = ids[0];
    let mut msg = vec![0u8; 4096];
    let mut small_frame = vec![0u8; 5000]; // a receive buffer smaller than the 9000-byte frame: stands in for a refused reservation
    let f = hello(1, 9000);
    let msgs: Vec<Vec<u8>> = i915_wire::frag::split(&f, Limits::KERNEL).unwrap().map(|m| m.to_vec()).collect();
    for m in &msgs {
        t.send_raw(id, m);
    }
    let c = tab.get_mut(id).unwrap();
    let first = c.pull(&mut t, &mut msg, &mut small_frame);
    assert!(matches!(first, Pulled::NoMemory { need: 9000, head: Some(h) } if h.request_id == 1), "{first:?}");
    assert_eq!(c.pull(&mut t, &mut msg, &mut small_frame), Pulled::Idle, "the rest of that frame is skipped, not parsed as new frames");
    assert_eq!(c.pull(&mut t, &mut msg, &mut small_frame), Pulled::Idle, "including the last message");
    // the next frame is taken normally
    t.send_raw(id, &hello(2, 100));
    assert_eq!(c.pull(&mut t, &mut msg, &mut small_frame), Pulled::Frame(100));
}

#[test]
fn pulling_names_every_bad_sequence_and_hands_back_the_broken_requests_header() {
    let mut t = FakeT::default();
    let (mut tab, ids) = table_with(&mut t, 1);
    let id = ids[0];
    let (mut msg, mut fr) = (vec![0u8; 4096], vec![0u8; 65576]);
    let f = hello(9, 9000);
    let m: Vec<Vec<u8>> = i915_wire::frag::split(&f, Limits::KERNEL).unwrap().map(|m| m.to_vec()).collect();
    let c = tab.get_mut(id).unwrap();
    t.send_raw(id, &m[0]);
    assert_eq!(c.pull(&mut t, &mut msg, &mut fr), Pulled::Partial);
    t.send_raw(id, &m[1][..10]);
    match c.pull(&mut t, &mut msg, &mut fr) {
        Pulled::Refused { err, head } => {
            assert_eq!(err, FragError::ShortFragment { index: 1, got: 10, cap: 4096 });
            assert_eq!(head.unwrap().request_id, 9, "the refusal can answer the request it broke");
        }
        o => panic!("{o:?}"),
    }
    // a refusal with no frame begun has no header to hand back
    let mut old = f[..30].to_vec();
    old[0] = b'X';
    t.send_raw(id, &old);
    t.send_raw(id, b"I915");
    assert!(matches!(c.pull(&mut t, &mut msg, &mut fr), Pulled::Legacy(30)), "no magic: the first generation's message");
    assert!(matches!(c.pull(&mut t, &mut msg, &mut fr), Pulled::Refused { err: FragError::FirstShort { got: 4 }, head: None }));
    // death comes after the queued messages, like the kernel's
    t.send_raw(id, &hello(1, 100));
    t.kill(id);
    assert_eq!(c.pull(&mut t, &mut msg, &mut fr), Pulled::Frame(100));
    assert_eq!(c.pull(&mut t, &mut msg, &mut fr), Pulled::Dead);
}

#[test]
fn the_parked_service_reassembles_answers_and_reaps_exactly_as_the_live_loop_does() {
    let t = FakeT::default();
    let mut p = Parked::new(t.clone(), Lines::default()).expect("buffers");
    t.0.borrow_mut().auto = true;
    let (a, b, c) = (t.connect(), t.connect(), t.connect());
    while p.step() != Wake::Closed {}
    assert_eq!((p.conns.len(), p.stats.accepted), (3, 3));

    // A sends a three-message frame while B and C send whole ones: A is answered once, when whole
    t.0.borrow_mut().auto = false;
    let m = i915_wire::frag::split(&hello(1, 10000), Limits::KERNEL).unwrap().map(|m| m.to_vec()).collect::<Vec<_>>();
    let push = |p: &mut Parked<FakeT, Lines>, id: ConnId, m: &[u8]| {
        t.0.borrow_mut().script.push_back(Ev::To(id, m.to_vec()));
        p.step();
    };
    push(&mut p, a, &m[0]);
    push(&mut p, b, &hello(2, 100));
    push(&mut p, a, &m[1]);
    assert_eq!(t.messages_sent(a), 0, "nothing is answered until the frame is whole");
    let two = i915_wire::frag::split(&hello(3, 5000), Limits::KERNEL).unwrap().map(|m| m.to_vec()).collect::<Vec<_>>();
    push(&mut p, c, &two[0]);
    push(&mut p, c, &two[1]);
    assert_eq!(parse(&t.drain_frames(c)[0]).hdr.request_id, 3);
    push(&mut p, a, &m[2]);
    for (id, rid) in [(a, 1u64), (b, 2)] {
        let r = parse(&t.drain_frames(id)[0]);
        assert_eq!((r.hdr.request_id, r.status), (rid, Status::Ok), "HELLO is answered OK, to its own client");
    }
    // a non-HELLO uAPI call is answered Unsupported with the reason, over the same connection
    push(&mut p, b, &frame(Opcode::GemCreate, 4, 0, &[0u8; 8]));
    assert_eq!(parse(&t.drain_frames(b)[0]).status, Status::Unsupported);

    // bad sequences are refused by name, the broken request answered
    let m = i915_wire::frag::split(&hello(5, 9000), Limits::KERNEL).unwrap().map(|m| m.to_vec()).collect::<Vec<_>>();
    push(&mut p, b, &m[0]);
    push(&mut p, b, &hello(6, 64)); // interleaved
    let r = parse(&t.drain_frames(b)[0]);
    assert_eq!((r.hdr.request_id, r.status), (5, Status::Malformed));
    assert_eq!(p.stats.refused, 1);
    assert!(p.log.0.iter().any(|l| l.contains("frame refused name=FRAG-INTERLEAVED")), "{:?}", p.log.0);

    // the first generation's four-byte protocol still answers
    t.drain_frames(c);
    push(&mut p, c, &[b'I', 1, 0]);
    assert_eq!(t.0.borrow().clients[&c].sent.last().unwrap(), &vec![b'i', 1, 0, 0]);

    // a dead client is released and only it
    t.kill(a);
    push(&mut p, a, &[0xDD, 1, 0, 0, 0, 0, 0, 0, 0]); // the kernel's tombstone: not a request, not an error
    assert_eq!(p.conns.len(), 3);
    t.0.borrow_mut().script.push_back(Ev::Wake(Wake::Request(a))); // and the dead endpoint stays READY: the next wake reaps
    p.step();
    assert_eq!((p.conns.len(), p.stats.reaped), (2, 1));
    assert_eq!(t.0.borrow().released, vec![a]);
}
