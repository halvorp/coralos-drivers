// SPDX-License-Identifier: GPL-2.0-only
//! The live loop over MANY clients at once: 50 concurrent `connect()` sessions interleaving fragmented frames on a fake kernel, replies
//! routed to the client that asked, one client's death freeing only its own session, a session private to its connection, and every
//! framing violation refused by name with nothing executed until a frame is whole.
#![cfg(feature = "p3-exec")]
mod exec_common;
mod net_common;
use coralos_hal::fake::FakeDma;
use exec_common::*;
use i915_device::live::{ConnId, Live, LiveStats, NoRegions, Step, Wake};
use i915_device::uapi::{bring_up_render, Hw, RenderConfig};
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_gt_core::emit::FlushQuirks;
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;
use i915_wire::msg::gem::*;
use i915_wire::msg::session::*;
use i915_wire::msg::Body;
use i915_wire::{errno, Opcode, Status, HEADER_BYTES};
use net_common::*;

const GIB: u64 = 1 << 30;
type L = Live<FakeT, NoRegions, Chip, FakeDma, Mem, Lines>;

fn world() -> (L, FakeT) {
    let (_chip, _mem, ex, mut gm) = built();
    let mut ex = ex;
    let mut log = Lines::default();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    let dev = UapiDevice::new(kbl(), 0x09, sseu, hz, 8 * GIB).unwrap();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    let cfg = RenderConfig { quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, stall_bound_ms: 1000 };
    let svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, cfg).unwrap();
    let t = FakeT::default();
    t.0.borrow_mut().now = 1;
    let live = Live::new(t.clone(), NoRegions, svc, ex, gm, log).unwrap();
    (live, t)
}

fn payload<B: Body>(b: &B) -> Vec<u8> {
    let mut v = vec![0u8; B::SIZE.max(64)];
    let n = b.to_payload(&mut v).unwrap();
    v.truncate(n);
    v
}

/// Run the loop until the script is empty and nothing is ready (the fake then reports the transport closed).
fn run(live: &mut L) {
    while live.step() != Step::Closed {}
}

/// Clients connect (auto mode: the announce is accepted first) and the loop settles.
fn connect_all(live: &mut L, t: &FakeT, n: usize) -> Vec<ConnId> {
    t.0.borrow_mut().auto = true;
    let ids: Vec<ConnId> = (0..n).map(|_| t.connect()).collect();
    run(live);
    assert_eq!(live.conns.len(), n);
    ids
}

/// The one frame the client got back (it sent one request, with a reply expected).
fn one_reply(t: &FakeT, id: ConnId) -> Resp {
    let mut f = t.drain_frames(id);
    assert_eq!(f.len(), 1, "exactly one reply frame for {id:?}, got {}", f.len());
    parse(&f.remove(0))
}

fn open(live: &mut L, t: &FakeT, id: ConnId, rid: u64) -> u64 {
    t.send_frame(id, &frame(Opcode::SessionOpen, rid, 0, &payload(&SessionOpenReq::default())));
    run(live);
    one_reply(t, id).body::<SessionOpenRep>().incarnation
}

#[test]
fn fifty_clients_interleave_fragmented_frames_and_every_reply_reaches_its_own_client() {
    const N: usize = 50;
    let (mut live, t) = world();
    let ids = connect_all(&mut live, &t, N);
    assert_eq!((live.stats.conns_accepted, live.stats.accept_wakes), (N as u64, N as u64));

    // every client's request stream: SESSION-OPEN, then three FRAGMENTED HELLO frames of different sizes (2..15 messages), then a GEM-CREATE
    // sized by its own number; the streams are dealt out one MESSAGE at a time, round-robin, so 50 fragmented frames are in flight at once
    t.0.borrow_mut().auto = false;
    let sizes = |c: usize| [4097usize + c * 3, 9000 + c * 101, 61440 - c];
    let mut streams: Vec<Vec<Vec<u8>>> = Vec::new();
    // the SESSION-OPENs first (their replies carry each incarnation), one message each, interleaved
    for (c, &id) in ids.iter().enumerate() {
        t.0.borrow_mut().script.push_back(Ev::To(id, frame(Opcode::SessionOpen, 1_000 * (c as u64 + 1), 0, &payload(&SessionOpenReq::default()))));
    }
    run(&mut live);
    let incs: Vec<u64> = ids.iter().map(|&id| one_reply(&t, id).hdr.session_incarnation).collect();
    let mut distinct = incs.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), N, "fifty sessions, fifty different incarnations");
    assert_eq!(live.svc.live_sessions(), N);
    live.stats = LiveStats::default();
    for (c, &id) in ids.iter().enumerate() {
        let mut msgs: Vec<Vec<u8>> = Vec::new();
        for (k, total) in sizes(c).into_iter().enumerate() {
            let f = frame(Opcode::Hello, 1_000 * (c as u64 + 1) + 1 + k as u64, incs[c], &vec![(c as u8).wrapping_add(k as u8); total - HEADER_BYTES]);
            msgs.extend(i915_wire::frag::split(&f, i915_wire::frag::Limits::KERNEL).unwrap().map(|m| m.to_vec()));
        }
        let create = frame(Opcode::GemCreate, 1_000 * (c as u64 + 1) + 9, incs[c], &payload(&GemCreateReq { size: 4096 * (c as u64 + 1) }));
        msgs.push(create);
        streams.push(msgs);
        let _ = id;
    }
    let total_msgs: usize = streams.iter().map(|s| s.len()).sum();
    let mut at = vec![0usize; N];
    let mut dealt = 0;
    while dealt < total_msgs {
        for c in 0..N {
            if at[c] < streams[c].len() {
                t.0.borrow_mut().script.push_back(Ev::To(ids[c], streams[c][at[c]].clone()));
                at[c] += 1;
                dealt += 1;
            }
        }
    }
    run(&mut live);

    // every client received exactly its own four replies, in order, with its own request ids and incarnation
    for (c, &id) in ids.iter().enumerate() {
        let frames = t.drain_frames(id);
        assert_eq!(frames.len(), 4, "client {c}: 3 HELLO replies and the GEM-CREATE reply");
        for (k, f) in frames.iter().enumerate() {
            let r = parse(f);
            let want = 1_000 * (c as u64 + 1) + if k < 3 { 1 + k as u64 } else { 9 };
            assert_eq!(r.hdr.request_id, want, "client {c} reply {k} is the answer to ITS request");
            assert_eq!(r.hdr.session_incarnation, incs[c], "client {c} reply {k} carries ITS session");
            assert_eq!((r.status, r.errno), (Status::Ok, 0), "client {c} reply {k}");
        }
    }
    assert_eq!(live.stats.calls, (N * 4) as u64, "every frame executed exactly once, and only when whole");
    assert_eq!(live.stats.refused, 0);
    assert_eq!(live.svc.gem().object_count(), N, "one object per session");
    assert_eq!(live.svc.live_sessions(), N);
    // nothing leaked into a table by fragmentation: no connection is left holding a partial frame (a new first message is taken whole)
    let waits_before = live.stats.waits;
    t.0.borrow_mut().script.push_back(Ev::To(ids[0], frame(Opcode::Hello, 99, incs[0], &[])));
    run(&mut live);
    assert_eq!(live.stats.waits, waits_before + 2, "one wake for the message, one that saw the transport close");
}

#[test]
fn one_clients_death_frees_only_its_own_session() {
    const N: usize = 50;
    let (mut live, t) = world();
    let ids = connect_all(&mut live, &t, N);
    let mut incs = vec![];
    for (c, &id) in ids.iter().enumerate() {
        let inc = open(&mut live, &t, id, 10 + c as u64);
        incs.push(inc);
        t.send_frame(id, &frame(Opcode::GemCreate, 500 + c as u64, inc, &payload(&GemCreateReq { size: 4096 })));
        run(&mut live);
        one_reply(&t, id).body::<GemCreateRep>();
    }
    assert_eq!((live.svc.live_sessions(), live.svc.gem().object_count(), live.conns.len()), (N, N, N));

    // client 17 dies in the middle of a fragmented frame: nothing of the half-received frame runs, its session and connection go
    let victim = ids[17];
    let big = frame(Opcode::Hello, 777, incs[17], &vec![7u8; 9000 - HEADER_BYTES]);
    let msgs: Vec<Vec<u8>> = i915_wire::frag::split(&big, i915_wire::frag::Limits::KERNEL).unwrap().map(|m| m.to_vec()).collect();
    t.send_raw(victim, &msgs[0]);
    run(&mut live);
    let calls = live.stats.calls;
    t.kill(victim);
    run(&mut live);
    assert_eq!(live.stats.calls, calls, "the dead client's half frame never executed");
    assert_eq!(live.stats.conns_reaped, 1);
    assert_eq!(live.stats.sessions_freed_by_peer_death, 1);
    assert_eq!((live.svc.live_sessions(), live.svc.gem().object_count(), live.conns.len()), (N - 1, N - 1, N - 1), "its session, its object and its connection - and nobody else's");
    assert_eq!(t.0.borrow().released, vec![victim], "its endpoints went back to the kernel, and only its");
    assert!(live.log.has("conn dead id="), "{:?}", live.log.0.last());

    // everyone else still works, including the neighbours of the dead one
    for c in [16usize, 18, 49, 0] {
        t.send_frame(ids[c], &frame(Opcode::GemCreate, 900 + c as u64, incs[c], &payload(&GemCreateReq { size: 4096 })));
        run(&mut live);
        let r = one_reply(&t, ids[c]);
        assert_eq!((r.status, r.errno, r.hdr.request_id), (Status::Ok, 0, 900 + c as u64));
    }
    assert_eq!(live.svc.gem().object_count(), N - 1 + 4);

    // its old incarnation is dead for everyone: a live client naming it is refused (see the privacy test), and a new client gets a NEW one
    let fresh = t.connect();
    run(&mut live);
    let inc = open(&mut live, &t, fresh, 1);
    assert!(!incs.contains(&inc), "an incarnation is never reused");
}

#[test]
fn a_session_belongs_to_its_connection_alone() {
    let (mut live, t) = world();
    let ids = connect_all(&mut live, &t, 3);
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    let inc_a = open(&mut live, &t, a, 1);
    t.send_frame(a, &frame(Opcode::GemCreate, 2, inc_a, &payload(&GemCreateReq { size: 4096 })));
    run(&mut live);
    one_reply(&t, a).body::<GemCreateRep>();
    assert_eq!(live.svc.gem().object_count(), 1);

    // B has no session of its own and names A's: refused as an unknown session, A's objects untouched
    t.send_frame(b, &frame(Opcode::GemCreate, 3, inc_a, &payload(&GemCreateReq { size: 4096 })));
    run(&mut live);
    let r = one_reply(&t, b);
    assert_eq!((r.status, r.errno), (Status::StaleSession, -errno::ENODEV), "indistinguishable from a session that does not exist");
    assert_eq!(live.svc.gem().object_count(), 1, "B created nothing in A's session");
    assert!(live.log.has("name=SESSION-NOT-ON-THIS-CONNECTION"));
    // B naming A's session to CLOSE it does not close it either
    t.send_frame(b, &frame(Opcode::SessionClose, 4, inc_a, &[]));
    run(&mut live);
    assert_eq!(one_reply(&t, b).status, Status::StaleSession);
    assert_eq!(live.svc.live_sessions(), 1);
    assert_eq!(live.stats.refused, 2);

    // B opens its own; a second SESSION-OPEN on the same connection is refused (one connection, one session)
    let inc_b = open(&mut live, &t, b, 5);
    assert_ne!(inc_a, inc_b);
    t.send_frame(b, &frame(Opcode::SessionOpen, 6, 0, &payload(&SessionOpenReq::default())));
    run(&mut live);
    let r = one_reply(&t, b);
    assert!(r.is_err(errno::EBUSY), "{:?} {}", r.status, r.errno);
    assert!(live.log.has("name=CONN-SESSION-EXISTS"));
    assert_eq!(live.svc.live_sessions(), 2);
    // B closes its session; then it may open another, and A is untouched throughout
    t.send_frame(b, &frame(Opcode::SessionClose, 7, inc_b, &[]));
    run(&mut live);
    assert_eq!(one_reply(&t, b).errno, 0);
    assert_eq!(live.svc.live_sessions(), 1);
    let inc_b2 = open(&mut live, &t, b, 8);
    assert!(inc_b2 != inc_a && inc_b2 != inc_b);
    // C with no session and incarnation 0 gets the service's own answer for a call without a session
    t.send_frame(c, &frame(Opcode::GemCreate, 9, 0, &payload(&GemCreateReq { size: 4096 })));
    run(&mut live);
    assert_eq!(one_reply(&t, c).status, Status::StaleSession);
    // when A dies, B keeps its session
    t.kill(a);
    run(&mut live);
    assert_eq!(live.svc.live_sessions(), 1);
    t.send_frame(b, &frame(Opcode::GemCreate, 10, inc_b2, &payload(&GemCreateReq { size: 4096 })));
    run(&mut live);
    assert_eq!(one_reply(&t, b).errno, 0);
}

#[test]
fn nothing_executes_until_the_frame_is_whole_and_every_bad_sequence_is_refused_by_name() {
    let (mut live, t) = world();
    let ids = connect_all(&mut live, &t, 2);
    let (a, b) = (ids[0], ids[1]);
    t.0.borrow_mut().auto = false;
    let hello = |rid: u64, total: usize| frame(Opcode::Hello, rid, 0, &vec![0u8; total - HEADER_BYTES]);
    let msgs = |f: &[u8]| -> Vec<Vec<u8>> { i915_wire::frag::split(f, i915_wire::frag::Limits::KERNEL).unwrap().map(|m| m.to_vec()).collect() };
    let step_with = |live: &mut L, id: ConnId, m: &[u8]| {
        t.0.borrow_mut().script.push_back(Ev::To(id, m.to_vec()));
        live.step();
    };

    // 3 messages: after 2 nothing has run and nothing was sent; the third runs it ONCE
    let m = msgs(&hello(1, 10000));
    step_with(&mut live, a, &m[0]);
    step_with(&mut live, a, &m[1]);
    assert_eq!((live.stats.calls, t.messages_sent(a)), (0, 0), "a half frame executes nothing and answers nothing");
    // another client's whole frame is served meanwhile, and does not disturb A's partial one
    step_with(&mut live, b, &hello(50, 100));
    assert_eq!(live.stats.calls, 1);
    step_with(&mut live, a, &m[2]);
    assert_eq!(live.stats.calls, 2);
    assert_eq!(one_reply(&t, a).hdr.request_id, 1);
    one_reply(&t, b);

    // SHORT: the second message of a three-message frame is cut short -> refused by name, the request answered, nothing run
    let m = msgs(&hello(2, 10000));
    step_with(&mut live, a, &m[0]);
    step_with(&mut live, a, &m[1][..1000]);
    let r = one_reply(&t, a);
    assert_eq!((r.hdr.request_id, r.status), (2, Status::Malformed), "the broken frame's own request is the one answered");
    assert!(live.log.has("frame refused name=FRAG-SHORT-FRAGMENT"), "{:?}", live.log.0.last());
    assert_eq!(live.stats.calls, 2, "nothing of it ran");

    // OVERLONG: the last message runs past the declared end
    let m = msgs(&hello(3, 5000));
    step_with(&mut live, a, &m[0]);
    let mut long = m[1].clone();
    long.push(0);
    step_with(&mut live, a, &long);
    let r = one_reply(&t, a);
    assert_eq!((r.hdr.request_id, r.status), (3, Status::Malformed));
    assert!(live.log.has("frame refused name=FRAG-OVERLONG"));

    // INTERLEAVED: a new frame's header arrives while the previous one is owed bytes
    let m = msgs(&hello(4, 9000));
    step_with(&mut live, a, &m[0]);
    step_with(&mut live, a, &hello(5, 64));
    let r = one_reply(&t, a);
    assert_eq!((r.hdr.request_id, r.status), (4, Status::Malformed));
    assert!(live.log.has("frame refused name=FRAG-INTERLEAVED"));
    assert_eq!(live.stats.refused, 3);
    assert_eq!(live.stats.calls, 2, "no refused sequence ran anything");

    // a SHORT FIRST message (less than a header, but with the magic) is refused by name; the stream is clean afterwards
    step_with(&mut live, a, &hello(6, 64)[..20]);
    assert_eq!(one_reply(&t, a).status, Status::Malformed);
    assert!(live.log.has("frame refused name=FRAG-FIRST-SHORT"));
    step_with(&mut live, a, &hello(7, 64));
    assert_eq!(one_reply(&t, a).hdr.request_id, 7, "after every refusal the next frame is taken whole");

    // the kernel's end-of-session tombstone between frames is not a request and not an error
    let refused = live.stats.refused;
    step_with(&mut live, b, &[0xDD, 1, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!((live.stats.refused, t.messages_sent(b)), (refused, 0));
}

#[test]
fn a_connect_the_device_cannot_track_is_refused_before_a_session_exists() {
    let (mut live, t) = world();
    // the heap has no room for another row: the loop stops listening (the wait says so), by name, and starts again when there is room
    t.0.borrow_mut().no_room = true;
    let c = t.connect();
    t.0.borrow_mut().script.push_back(Ev::Wake(Wake::Deadline));
    live.step();
    assert_eq!(t.0.borrow().waits.last().unwrap().1, false, "the wait is told not to accept");
    assert!(live.log.has("accept paused name=CONN-NOMEM"));
    assert_eq!(live.conns.len(), 0);
    assert_eq!(t.0.borrow().announces.len(), 1, "the announce stays queued for the kernel to refuse when the ring fills");
    t.0.borrow_mut().no_room = false;
    t.0.borrow_mut().script.push_back(Ev::Wake(Wake::Accept));
    live.step();
    assert_eq!(t.0.borrow().waits.last().unwrap().1, true);
    assert_eq!(live.conns.len(), 1);
    let _ = c;
    // an announce the transport refuses is counted and named, and admits nothing
    t.0.borrow_mut().refuse_next = Some("ANNOUNCE-MALFORMED");
    t.0.borrow_mut().script.push_back(Ev::Wake(Wake::Accept));
    live.step();
    assert_eq!((live.stats.connects_refused, live.conns.len()), (1, 1));
    assert!(live.log.has("connect refused name=ANNOUNCE-MALFORMED"));
}

#[test]
fn a_slow_client_holds_only_itself_and_its_fragmented_reply_resumes_in_order() {
    let (mut live, t) = world();
    let ids = connect_all(&mut live, &t, 2);
    let (slow, fast) = (ids[0], ids[1]);
    t.0.borrow_mut().auto = false;
    let inc_s = {
        t.0.borrow_mut().auto = true;
        let i = open(&mut live, &t, slow, 1);
        t.0.borrow_mut().auto = false;
        i
    };
    // the slow client's ring is full: its next reply is kept, its requests are held; the other client is served meanwhile
    t.0.borrow_mut().clients.get_mut(&slow).unwrap().ring_full = 2; // the reply's own send, and the same step's retry
    t.0.borrow_mut().script.push_back(Ev::To(slow, frame(Opcode::GemCreate, 2, inc_s, &payload(&GemCreateReq { size: 4096 }))));
    live.step();
    assert!(t.0.borrow().clients[&slow].held && live.conns.iter_mut().any(|c| c.has_pending()));
    t.0.borrow_mut().script.push_back(Ev::To(fast, frame(Opcode::Hello, 3, 0, &[])));
    live.step();
    assert_eq!(t.drain_frames(fast).len(), 1, "the other client is answered while the slow one is held");
    // the retry timer drains the held reply; the client's requests are resumed
    t.0.borrow_mut().script.push_back(Ev::At(Wake::Deadline, 5));
    live.step();
    assert!(!t.0.borrow().clients[&slow].held);
    assert_eq!(one_reply(&t, slow).hdr.request_id, 2);
}
