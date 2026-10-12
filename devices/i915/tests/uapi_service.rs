// SPDX-License-Identifier: GPL-2.0-only
//! The uAPI service on a simulated GPU, driven ONLY through wire frames (the way the closed shim will drive it): a session creates
//! buffers, maps them through leases, submits an `MI_STORE_DWORD` batch through EXECBUFFER2, waits on the out-syncobj and reads what the
//! batch stored. The simulated GPU does nothing by itself (exec_engine.rs): the test plays the hardware when it wants progress.
#![cfg(feature = "p3-exec")]
mod exec_common;
use exec_common::*;
use i915_device::exec::dma::ObjKind;
use i915_device::uapi::{bring_up_render, Handled, Hw, RenderConfig, UapiService};
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_gt_core::emit::FlushQuirks;
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;
use i915_wire::msg::ctx::*;
use i915_wire::msg::device::*;
use i915_wire::msg::exec::{self, ExecObject, ExecReply};
use i915_wire::msg::gem::*;
use i915_wire::msg::prime::*;
use i915_wire::msg::session::*;
use i915_wire::msg::sync::{self, *};
use i915_syncobj_core::CREATE_SIGNALED;
use i915_wire::msg::Body;
use i915_wire::{errno, Header, Opcode, Status, HEADER_BYTES};

const BATCH_VA: u64 = 0x1000_0000_0000;
const RESULT_VA: u64 = 0x2000_0000;
const VALUE: u32 = 0xc0ff_ee42;
const MI_STORE_DWORD_IMM_GEN4: u32 = (0x20 << 23) | 2; // intel_gpu_commands.h:136
const MI_BATCH_BUFFER_END: u32 = 0x0a << 23; // intel_gpu_commands.h:62
const PINNED: u64 = 1 << 4;
const SUPPORTS_48B: u64 = 1 << 3;
const WRITE: u64 = 1 << 2;
const FENCE_SIGNAL: u32 = 2;
const FENCE_WAIT: u32 = 1;
const GIB: u64 = 1 << 30;

struct W {
    chip: Chip,
    mem: Mem,
    ex: i915_device::exec::Executor<Chip>,
    gm: Gm,
    log: Lines,
    svc: UapiService,
    now: u64,
    csb: u32,
    rid: u64,
}

macro_rules! hw {
    ($w:ident) => {
        Hw { ex: &mut $w.ex, gm: &mut $w.gm, log: &mut $w.log, now_ms: $w.now }
    };
}

fn world() -> W {
    let (chip, mem, mut ex, mut gm) = built();
    let mut log = Lines::default();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    let dev = UapiDevice::new(kbl(), 0x09, sseu, hz, 8 * GIB).unwrap();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    let cfg = RenderConfig { quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, stall_bound_ms: 1000 };
    let svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, cfg).unwrap();
    W { chip, mem, ex, gm, log, svc, now: 1, csb: 0, rid: 0 }
}

/// What a call answered.
struct Resp {
    hdr: Header,
    status: Status,
    errno: i32,
    body: Vec<u8>,
    /// status + errno + body, as the reply payload
    payload: Vec<u8>,
}

impl Resp {
    fn body<B: Body>(&self) -> B {
        assert_eq!((self.status, self.errno), (Status::Ok, 0), "the call failed: {:?} errno {}", self.status, self.errno);
        B::from_payload(&self.body).unwrap()
    }
    fn is_err(&self, e: i32) -> bool {
        self.status == Status::Ok && self.errno == -e
    }
}

fn parse(frame: &[u8]) -> Resp {
    let (hdr, p) = Header::decode(frame).unwrap();
    assert!(hdr.is_reply());
    let (status, errno) = (Status::from_u32(u32::from_le_bytes(p[0..4].try_into().unwrap())).unwrap(), i32::from_le_bytes(p[4..8].try_into().unwrap()));
    Resp { hdr, status, errno, body: p[8..].to_vec(), payload: p.to_vec() }
}

/// Send a raw payload. `None` = the call parked.
fn send_raw(w: &mut W, inc: u64, op: Opcode, payload: &[u8]) -> Option<Resp> {
    w.rid += 1;
    let mut f = vec![0u8; HEADER_BYTES + payload.len()];
    f[HEADER_BYTES..].copy_from_slice(payload);
    Header::request(op, w.rid, inc, payload.len() as u32).encode(&mut f).unwrap();
    let mut out = vec![0u8; 80_000];
    match w.svc.handle(&mut hw!(w), &f, &mut out).unwrap() {
        Handled::Reply(n) => {
            let r = parse(&out[..n]);
            assert_eq!((r.hdr.opcode, r.hdr.request_id), (op, w.rid), "the reply echoes the request");
            Some(r)
        }
        Handled::Parked => None,
    }
}

fn call<B: Body>(w: &mut W, inc: u64, op: Opcode, req: &B) -> Resp {
    let mut p = vec![0u8; 4096];
    let n = req.to_payload(&mut p).unwrap();
    send_raw(w, inc, op, &p[..n]).expect("the call parked")
}

fn open(w: &mut W) -> u64 {
    let r = call(w, 0, Opcode::SessionOpen, &SessionOpenReq::default());
    assert_eq!(r.hdr.session_incarnation, r.body::<SessionOpenRep>().incarnation, "the reply header carries the new incarnation");
    r.body::<SessionOpenRep>().incarnation
}

fn create(w: &mut W, inc: u64, size: u64) -> u32 {
    call(w, inc, Opcode::GemCreate, &GemCreateReq { size }).body::<GemCreateRep>().handle
}

/// A CPU lease on the whole object.
fn lease(w: &mut W, inc: u64, handle: u32, len: u64) -> GemMmapRep {
    let off = call(w, inc, Opcode::GemMmapOffset, &GemMmapOffsetReq { handle, flags: 2, ..Default::default() }).body::<GemMmapOffsetRep>();
    call(w, inc, Opcode::GemMmapOpen, &GemMmapOpenReq { offset: off.offset, len }).body::<GemMmapRep>()
}

fn syncobj(w: &mut W, inc: u64, flags: u32) -> u32 {
    call(w, inc, Opcode::SyncobjCreate, &SyncobjCreateReq { flags, ..Default::default() }).body::<SyncobjHandleRep>().handle
}

fn write_batch(w: &mut W, batch_lease: &GemMmapRep) {
    let va = RESULT_VA;
    for (i, d) in [MI_STORE_DWORD_IMM_GEN4, va as u32, (va >> 32) as u32, VALUE, MI_BATCH_BUFFER_END].iter().enumerate() {
        w.gm.store32(batch_lease.grant_addr + 4 * i as u64, *d).unwrap();
    }
}

/// An EXECBUFFER2 payload: `[result, batch]`, both softpinned, the batch last; `fences` are `(syncobj, flags)`.
fn exec_payload(result: u32, batch: u32, fences: &[(u32, u32)], timeline: Option<(u32, u64)>, ctx: u32) -> Vec<u8> {
    let objs = [
        ExecObject { handle: result, offset: RESULT_VA, flags: PINNED | SUPPORTS_48B | WRITE, ..Default::default() },
        ExecObject { handle: batch, offset: BATCH_VA, flags: PINNED | SUPPORTS_48B, ..Default::default() },
    ];
    let mut flags = exec::flags::NO_RELOC;
    let fa: Vec<exec::Fence> = fences.iter().map(|(h, f)| exec::Fence { handle: *h, flags: *f }).collect();
    let mut ext = vec![0u8; 256];
    let mut ext_n = 0;
    let mut hdr = exec::ExecHeader { rsvd1: ctx as u64, batch_len: 24, ..Default::default() };
    if let Some((h, point)) = timeline {
        flags |= exec::flags::USE_EXTENSIONS;
        let mut wr = i915_wire::codec::Writer::new(&mut ext);
        exec::put_timeline_fences(&mut wr, &[exec::Fence { handle: h, flags: FENCE_SIGNAL }], &[point]).unwrap();
        ext_n = wr.pos();
        hdr.ext_count = 1;
    } else if !fences.is_empty() {
        flags |= exec::flags::FENCE_ARRAY;
    }
    hdr.flags = flags;
    let mut out = vec![0u8; 4096];
    let n = exec::encode_request(&mut out, hdr, &objs, &[], &fa, &ext[..ext_n]).unwrap();
    out.truncate(n);
    out
}

fn submit(w: &mut W, inc: u64, payload: &[u8]) -> (Resp, Option<exec::ExecRepHead>) {
    let r = send_raw(w, inc, Opcode::Execbuffer2, payload).unwrap();
    let head = (r.errno == 0 && r.status == Status::Ok).then(|| ExecReply::decode(&r.payload).unwrap().head);
    (r, head)
}

/// The hardware picks up the oldest submission and completes it: the CSB promotes then retires, the context's timeline reaches `seqno`.
fn gpu_complete(w: &mut W, timeline_off: u32, seqno: u32) {
    let hwsp = *w.gm.alloc.grants().find(|g| g.kind == ObjKind::Hwsp).unwrap();
    let first = w.csb;
    let entry = |i: u32| hwsp.cpu_mapping + (0x10 + 2 * (i % 6)) as u64 * 4;
    w.mem.put(entry(first), 1);
    w.mem.put(entry(first) + 4, 0);
    w.mem.put(entry(first + 1), 1 << 3);
    w.mem.put(entry(first + 1) + 4, 0);
    w.mem.put(hwsp.cpu_mapping + timeline_off as u64, seqno);
    w.mem.put(hwsp.cpu_mapping + 0x1f * 4, (first + 1) % 6);
    w.csb += 2;
}

fn poll(w: &mut W) {
    w.now += 1;
    w.svc.poll(&mut hw!(w));
}

fn refused(w: &W, name: &str) -> bool {
    w.log.0.iter().any(|l| l.contains("refused") && l.contains(name))
}

// ---- the end-to-end path ----------------------------------------------------------------------------------------------------

#[test]
fn a_session_stores_a_dword_through_execbuffer2_and_reads_it_back_through_its_lease() {
    let mut w = world();
    let inc = open(&mut w);
    let (batch, result) = (create(&mut w, inc, 4096), create(&mut w, inc, 4096));
    let (bl, rl) = (lease(&mut w, inc, batch, 4096), lease(&mut w, inc, result, 4096));
    assert_eq!((bl.len, rl.len, bl.cache), (4096, 4096, cache::WRITE_BACK));
    assert_ne!(bl.grant_addr, rl.grant_addr);
    write_batch(&mut w, &bl);
    assert_eq!(w.gm.load32(rl.grant_addr).unwrap(), 0, "a new object reads as zeroes");

    let out = syncobj(&mut w, inc, 0);
    let payload = exec_payload(result, batch, &[(out, FENCE_SIGNAL)], None, 0);
    let (r, head) = submit(&mut w, inc, &payload);
    let head = head.unwrap_or_else(|| panic!("EXECBUFFER2 refused: errno {} log {:?}", r.errno, w.log.0.last()));
    assert_eq!(head.flags & exec::REP_SUBMITTED, exec::REP_SUBMITTED, "the request went to the engine");
    assert_eq!(head.seqno, 2, "a timeline with an initial breadcrumb advances by two");
    assert_eq!(w.chip.writes_to(0x2230).len(), 4, "four ELSP writes: one submission");
    assert_eq!(w.svc.requests_in_flight(), 1);
    // both objects sit exactly where they were softpinned
    let ex = ExecReply::decode(&r.payload).unwrap();
    assert_eq!((ex.offset(0), ex.offset(1)), (Some(RESULT_VA), Some(BATCH_VA)));

    // the out-syncobj is pending: a poll-wait says ETIME, a blocking wait parks
    let mut wait = vec![0u8; 64];
    let n = WaitReq::encode(&mut wait, 0, 0, &[out], None).unwrap();
    assert!(send_raw(&mut w, inc, Opcode::SyncobjWait, &wait[..n]).unwrap().is_err(errno::ETIME));
    let n = WaitReq::encode(&mut wait, 0, 1_000_000_000, &[out], None).unwrap();
    assert!(send_raw(&mut w, inc, Opcode::SyncobjWait, &wait[..n]).is_none(), "the wait parks");
    let wait_rid = w.rid;
    assert_eq!(w.svc.parked_calls(), 1);
    assert!(w.svc.take_ready().is_none());

    // the hardware runs the batch: it stores VALUE at the result address and the timeline reaches the request's seqno
    let tl = w.svc.timeline_offsets()[0];
    w.gm.store32(rl.grant_addr, VALUE).unwrap(); // MI_STORE_DWORD_IMM executed
    gpu_complete(&mut w, tl, head.seqno);
    poll(&mut w);
    assert_eq!(w.svc.requests_in_flight(), 0);
    let ready = parse(&w.svc.take_ready().expect("the parked wait is answered"));
    assert_eq!((ready.hdr.opcode, ready.status, ready.errno), (Opcode::SyncobjWait, Status::Ok, 0));
    assert_eq!(ready.hdr.request_id, wait_rid, "the same request id the wait carried");
    assert_eq!(ready.body::<SyncobjWaitRep>().first_signaled, 0);
    assert_eq!(w.gm.load32(rl.grant_addr).unwrap(), VALUE, "the stored value, through the lease");
    assert_eq!(call(&mut w, inc, Opcode::GemBusy, &GemBusyReq { handle: result, ..Default::default() }).body::<GemBusyRep>().busy, 0);
    assert_eq!(call(&mut w, inc, Opcode::GemWait, &GemWaitReq { handle: result, flags: 0, timeout_ns: -1 }).errno, 0);
    assert_eq!(w.ex.stats().refused, 0, "no register write outside the allow-list");
}

#[test]
fn a_blocking_gem_wait_is_answered_when_the_request_completes_or_times_out() {
    let mut w = world();
    let inc = open(&mut w);
    let (batch, result) = (create(&mut w, inc, 4096), create(&mut w, inc, 4096));
    let bl = lease(&mut w, inc, batch, 4096);
    write_batch(&mut w, &bl);
    let (_, head) = submit(&mut w, inc, &exec_payload(result, batch, &[], None, 0));
    let head = head.unwrap();
    // busy: the object is in use by a render request (bit 16 + class | class + 1 for a writer)
    let busy = call(&mut w, inc, Opcode::GemBusy, &GemBusyReq { handle: result, ..Default::default() }).body::<GemBusyRep>().busy;
    assert_eq!(busy, (1 << 16) | 1);
    // zero timeout: -ETIME at once; a bounded wait parks and times out on the device clock
    assert!(call(&mut w, inc, Opcode::GemWait, &GemWaitReq { handle: result, flags: 0, timeout_ns: 0 }).is_err(errno::ETIME));
    assert!(send_raw(&mut w, inc, Opcode::GemWait, &GemWaitReq { handle: result, flags: 0, timeout_ns: 5_000_000 }.to_payload_vec()).is_none());
    assert!(send_raw(&mut w, inc, Opcode::GemWait, &GemWaitReq { handle: result, flags: 0, timeout_ns: -1 }.to_payload_vec()).is_none());
    assert_eq!(w.svc.parked_calls(), 2);
    w.now += 10; // past the 5 ms deadline
    w.svc.poll(&mut hw!(w));
    let first = parse(&w.svc.take_ready().expect("the timed-out wait is answered"));
    assert!(first.is_err(errno::ETIME), "errno {}", first.errno);
    assert_eq!(w.svc.parked_calls(), 1, "the unbounded wait is still parked");
    let tl = w.svc.timeline_offsets()[0];
    gpu_complete(&mut w, tl, head.seqno);
    poll(&mut w);
    let second = parse(&w.svc.take_ready().expect("the unbounded wait is answered"));
    assert_eq!((second.status, second.errno), (Status::Ok, 0));
    assert_eq!(w.svc.parked_calls(), 0);
}

trait ToVec: Body {
    fn to_payload_vec(&self) -> Vec<u8> {
        let mut v = vec![0u8; Self::SIZE];
        self.to_payload(&mut v).unwrap();
        v
    }
}
impl<T: Body> ToVec for T {}

// ---- isolation -----------------------------------------------------------------------------------------------------------------

#[test]
fn two_sessions_cannot_touch_each_others_handles_leases_or_syncobjs() {
    let mut w = world();
    let (a, b) = (open(&mut w), open(&mut w));
    assert_ne!(a, b);
    let ha = create(&mut w, a, 4096);
    let ha2 = create(&mut w, a, 4096); // handle 2: B will have no handle 2 when it presents it
    let la = lease(&mut w, a, ha, 4096);
    let sa = syncobj(&mut w, a, CREATE_SIGNALED);
    // B presents A's handle everywhere a handle goes: a NAMED refusal, errno -ENOENT, and nothing of A's changes
    assert!(call(&mut w, b, Opcode::GemClose, &GemCloseReq { handle: ha, ..Default::default() }).is_err(errno::EINVAL), "drm_gem_handle_delete: a handle that is not ours is -EINVAL");
    assert!(call(&mut w, b, Opcode::GemMmapOffset, &GemMmapOffsetReq { handle: ha, flags: 2, ..Default::default() }).is_err(errno::ENOENT));
    assert!(call(&mut w, b, Opcode::GemSetDomain, &GemSetDomainReq { handle: ha, read_domains: 1, write_domain: 0, ..Default::default() }).is_err(errno::ENOENT));
    assert!(call(&mut w, b, Opcode::GemBusy, &GemBusyReq { handle: ha, ..Default::default() }).is_err(errno::ENOENT));
    assert!(call(&mut w, b, Opcode::PrimeHandleToFd, &PrimeHandleToFdReq { handle: ha, flags: 0 }).is_err(errno::ENOENT));
    assert!(refused(&w, "CROSS-SESSION-HANDLE"), "the refusal is named: {:?}", w.log.0);
    let rb = create(&mut w, b, 4096);
    let payload = exec_payload(ha2, rb, &[], None, 0);
    let r = submit(&mut w, b, &payload).0; assert!(r.is_err(errno::ENOENT), "A object is not an exec object of B: errno {} log {:?}", r.errno, w.log.0.last());
    // A's lease and A's syncobj
    assert!(call(&mut w, b, Opcode::GemMmapClose, &GemMmapCloseReq { lease: la.lease, ..Default::default() }).is_err(errno::ENOENT));
    assert!(refused(&w, "LEASE-UNKNOWN"));
    assert!(call(&mut w, b, Opcode::SyncobjDestroy, &SyncobjDestroyReq { handle: sa, pad: 0 }).is_err(errno::EINVAL));
    let mut arr = [0u8; 32];
    let n = ArrayReq::encode(&mut arr, 0, &[sa], None).unwrap();
    assert!(send_raw(&mut w, b, Opcode::SyncobjSignal, &arr[..n]).unwrap().is_err(errno::ENOENT));
    // ...and all of it still works for A
    assert_eq!(call(&mut w, a, Opcode::GemBusy, &GemBusyReq { handle: ha, ..Default::default() }).errno, 0);
    assert!(call(&mut w, a, Opcode::GemMmapClose, &GemMmapCloseReq { lease: la.lease, ..Default::default() }).errno == 0);
    // a dead or invented incarnation is StaleSession, whatever the call
    for inc in [0xdead_beef, 0] {
        let r = call(&mut w, inc, Opcode::GemBusy, &GemBusyReq { handle: ha, ..Default::default() });
        assert_eq!((r.status, r.errno), (Status::StaleSession, -errno::ENODEV));
    }
    // PRIME is the one sanctioned way across: the token, not the handle
    let tok = call(&mut w, a, Opcode::PrimeHandleToFd, &PrimeHandleToFdReq { handle: ha, flags: 0 }).body::<PrimeTokenRep>().token;
    let hb = call(&mut w, b, Opcode::PrimeFdToHandle, &PrimeFdToHandleReq { token: tok }).body::<PrimeHandleRep>().handle;
    assert_eq!(call(&mut w, b, Opcode::GemBusy, &GemBusyReq { handle: hb, ..Default::default() }).errno, 0);
    assert!(call(&mut w, b, Opcode::PrimeFdToHandle, &PrimeFdToHandleReq { token: tok + 100 }).is_err(errno::EINVAL));
}

#[test]
fn prime_keeps_the_object_alive_after_the_exporter_closes_and_dies() {
    let mut w = world();
    let (a, b) = (open(&mut w), open(&mut w));
    let ha = create(&mut w, a, 8192);
    let la = lease(&mut w, a, ha, 8192);
    w.gm.store32(la.grant_addr, 0x1234_5678).unwrap();
    let tok = call(&mut w, a, Opcode::PrimeHandleToFd, &PrimeHandleToFdReq { handle: ha, flags: 0 }).body::<PrimeTokenRep>().token;
    let hb = call(&mut w, b, Opcode::PrimeFdToHandle, &PrimeFdToHandleReq { token: tok }).body::<PrimeHandleRep>().handle;
    let lb = lease(&mut w, b, hb, 8192);
    assert_eq!(lb.grant_addr, la.grant_addr, "one object, one memory: the importer maps the same pages");
    w.svc.session_dead(&mut hw!(w), a);
    assert_eq!(w.svc.live_primes(), 0, "the token died with its exporter");
    assert!(call(&mut w, b, Opcode::PrimeFdToHandle, &PrimeFdToHandleReq { token: tok }).is_err(errno::EINVAL));
    assert_eq!(w.gm.load32(lb.grant_addr).unwrap(), 0x1234_5678, "B's handle still names live memory");
    assert_eq!(w.svc.gem().object_count(), 1);
    w.svc.session_dead(&mut hw!(w), b);
    poll(&mut w);
    assert_eq!(w.svc.gem().object_count(), 0);
    assert_eq!(w.svc.live_object_pages(), 0);
}

// ---- death and leaks -----------------------------------------------------------------------------------------------------------

fn snapshot(w: &W) -> [usize; 14] {
    let s = &w.svc;
    [
        s.live_sessions(),
        s.gem().session_count(),
        s.gem().object_count(),
        s.gem().vm_count(),
        s.live_leases(),
        s.live_primes(),
        s.live_hw_contexts(),
        s.live_vm_records(),
        s.live_object_pages(),
        s.requests_in_flight(),
        s.sync().live_syncobjs(),
        s.sync().live_fences(),
        s.sync().live_exports(),
        s.pool().pages_out(),
    ]
}

/// One session that uses everything: objects, leases, contexts, VMs, syncobjs, sync_files, exports, PRIME, a finished submission.
fn use_everything(w: &mut W) -> u64 {
    let inc = open(w);
    let (batch, result) = (create(w, inc, 4096), create(w, inc, 8192));
    let bl = lease(w, inc, batch, 4096);
    let _rl = lease(w, inc, result, 4096);
    write_batch(w, &bl);
    let c1 = call(w, inc, Opcode::ContextCreate, &ContextCreateReq {}).body::<ContextRep>().ctx_id;
    let vm = call(w, inc, Opcode::VmCreate, &VmCreateReq::default()).body::<VmCreateRep>().vm_id;
    assert!(vm >= 1 && c1 >= 1);
    let (s1, s2) = (syncobj(w, inc, 0), syncobj(w, inc, CREATE_SIGNALED));
    let tok = call(w, inc, Opcode::SyncobjHandleToFd, &SyncobjHandleToFdReq { handle: s2, flags: 0, point: 0 }).body::<SyncobjTokenRep>().token;
    assert_ne!(tok, 0);
    let sf = call(w, inc, Opcode::SyncobjHandleToFd, &SyncobjHandleToFdReq { handle: s2, flags: sync::flags::HANDLE_TO_FD_EXPORT_SYNC_FILE, point: 0 }).body::<SyncobjTokenRep>().token;
    assert_ne!(sf, 0);
    call(w, inc, Opcode::PrimeHandleToFd, &PrimeHandleToFdReq { handle: result, flags: 0 }).body::<PrimeTokenRep>();
    // a finished submission on the default context and on a created one
    for ctx in [0, c1] {
        let (_, head) = submit(w, inc, &exec_payload(result, batch, &[(s1, FENCE_SIGNAL)], None, ctx));
        let head = head.unwrap_or_else(|| panic!("exec on ctx {ctx} refused: {:?}", w.log.0.last()));
        let tl = *w.svc.timeline_offsets().last().unwrap();
        gpu_complete(w, tl, head.seqno);
        poll(w);
    }
    // a timeline syncobj with points
    let t = syncobj(w, inc, 0);
    let mut arr = [0u8; 64];
    let n = ArrayReq::encode(&mut arr, 0, &[t, t], Some(&[1, 2])).unwrap();
    assert_eq!(send_raw(w, inc, Opcode::SyncobjTimelineSignal, &arr[..n]).unwrap().errno, 0);
    inc
}

#[test]
fn killing_a_session_frees_every_handle_context_vm_lease_syncobj_and_page() {
    let mut w = world();
    let before = snapshot(&w);
    let inc = use_everything(&mut w);
    let busy = snapshot(&w);
    assert!(busy.iter().zip(before.iter()).filter(|(a, b)| a > b).count() >= 9, "the session really held things: {busy:?} vs {before:?}");
    assert!(w.svc.live_hw_contexts() >= 2 && w.svc.live_vm_records() >= 3 && w.svc.live_leases() == 2 && w.svc.live_primes() == 1);
    w.svc.session_dead(&mut hw!(w), inc);
    for _ in 0..3 {
        poll(&mut w);
    }
    let after = snapshot(&w);
    assert_eq!(after, before, "everything the session held is gone: {after:?} vs baseline {before:?}");
    assert_eq!(w.svc.live_hw_contexts(), 0);
    // and the dead session is dead: its incarnation answers StaleSession
    assert_eq!(call(&mut w, inc, Opcode::GemBusy, &GemBusyReq::default()).status, Status::StaleSession);
}

#[test]
fn churning_sessions_recycles_memory_instead_of_growing() {
    let mut w = world();
    let inc = use_everything(&mut w);
    w.svc.session_dead(&mut hw!(w), inc);
    for _ in 0..3 {
        poll(&mut w);
    }
    let (host_pages, left) = (w.svc.pool().pages_from_host(), w.gm.alloc.left());
    for _ in 0..12 {
        let inc = use_everything(&mut w);
        w.svc.session_dead(&mut hw!(w), inc);
        for _ in 0..3 {
            poll(&mut w);
        }
    }
    assert_eq!(w.svc.pool().pages_lost(), 0);
    assert_eq!(w.svc.pool().pages_from_host(), host_pages, "no new pages were taken from the host after the first cycle");
    assert_eq!(w.gm.alloc.left(), left, "the DMA endowment is untouched by further cycles... except context images and rings, which are pooled too");
    assert_eq!(w.svc.pool().pages_out(), 0);
}

#[test]
fn a_session_that_dies_with_work_in_flight_is_freed_when_the_work_retires() {
    let mut w = world();
    let before = snapshot(&w);
    let inc = open(&mut w);
    let (batch, result) = (create(&mut w, inc, 4096), create(&mut w, inc, 4096));
    let bl = lease(&mut w, inc, batch, 4096);
    write_batch(&mut w, &bl);
    let head = submit(&mut w, inc, &exec_payload(result, batch, &[], None, 0)).1.unwrap();
    w.svc.session_dead(&mut hw!(w), inc);
    assert!(w.svc.gem().object_count() >= 2, "the objects live until the GPU is done with them");
    assert_eq!(w.svc.requests_in_flight(), 1);
    let tl = w.svc.timeline_offsets()[0];
    gpu_complete(&mut w, tl, head.seqno);
    for _ in 0..3 {
        poll(&mut w);
    }
    assert_eq!(snapshot(&w), before);
    assert_eq!(w.svc.live_hw_contexts(), 0);
}

// ---- ordering: a request waits for its fence -------------------------------------------------------------------------------------

#[test]
fn a_request_waiting_on_an_unsignalled_fence_is_held_until_the_fence_signals() {
    let mut w = world();
    let inc = open(&mut w);
    let (batch, result) = (create(&mut w, inc, 4096), create(&mut w, inc, 4096));
    let bl = lease(&mut w, inc, batch, 4096);
    write_batch(&mut w, &bl);
    let (s_a, s_b) = (syncobj(&mut w, inc, 0), syncobj(&mut w, inc, 0));
    // A signals s_a; B waits on s_a and signals s_b
    let a = submit(&mut w, inc, &exec_payload(result, batch, &[(s_a, FENCE_SIGNAL)], None, 0)).1.unwrap();
    assert_eq!(w.chip.writes_to(0x2230).len(), 4);
    let b = submit(&mut w, inc, &exec_payload(result, batch, &[(s_a, FENCE_WAIT), (s_b, FENCE_SIGNAL)], None, 0)).1.unwrap();
    assert_eq!(b.flags & exec::REP_SUBMITTED, 0, "B is queued behind A's fence");
    assert_eq!(w.svc.gated_requests(), 1);
    assert_eq!(w.chip.writes_to(0x2230).len(), 4, "B has not been written to the ELSP");
    // A completes: its fence signals, B is released and written to the ELSP in the same poll
    let tl = w.svc.timeline_offsets()[0];
    gpu_complete(&mut w, tl, a.seqno);
    poll(&mut w);
    poll(&mut w);
    assert_eq!(w.svc.gated_requests(), 0);
    assert_eq!(w.chip.writes_to(0x2230).len(), 8, "B started once its fence signalled");
    gpu_complete(&mut w, tl, b.seqno);
    poll(&mut w);
    let mut wait = [0u8; 64];
    let n = WaitReq::encode(&mut wait, 0, 0, &[s_b], None).unwrap();
    assert_eq!(send_raw(&mut w, inc, Opcode::SyncobjWait, &wait[..n]).unwrap().errno, 0);
    assert_eq!(w.svc.requests_in_flight(), 0);
}

#[test]
fn timeline_points_are_signalled_by_retirement_and_queried() {
    let mut w = world();
    let inc = open(&mut w);
    let (batch, result) = (create(&mut w, inc, 4096), create(&mut w, inc, 4096));
    let bl = lease(&mut w, inc, batch, 4096);
    write_batch(&mut w, &bl);
    let t = syncobj(&mut w, inc, 0);
    let head = submit(&mut w, inc, &exec_payload(result, batch, &[], Some((t, 5)), 0)).1.unwrap();
    let mut arr = [0u8; 32];
    let n = ArrayReq::encode(&mut arr, 0, &[t], None).unwrap();
    let q = |w: &mut W, flags: u32| {
        let mut a = [0u8; 32];
        let n = ArrayReq::encode(&mut a, flags, &[t], None).unwrap();
        let r = send_raw(w, inc, Opcode::SyncobjQuery, &a[..n]).unwrap();
        sync::decode_query_reply(&r.payload).unwrap().1.get(0).unwrap()
    };
    let _ = n;
    assert_eq!(q(&mut w, sync::flags::QUERY_LAST_SUBMITTED), 5);
    assert_eq!(q(&mut w, 0), 0, "point 5 is not signalled yet");
    let mut wait = [0u8; 64];
    let n = WaitReq::encode(&mut wait, sync::flags::WAIT_AVAILABLE, 0, &[t], Some(&[5])).unwrap();
    assert_eq!(send_raw(&mut w, inc, Opcode::SyncobjTimelineWait, &wait[..n]).unwrap().errno, 0, "the point exists");
    let n = WaitReq::encode(&mut wait, 0, 0, &[t], Some(&[5])).unwrap();
    assert!(send_raw(&mut w, inc, Opcode::SyncobjTimelineWait, &wait[..n]).unwrap().is_err(errno::ETIME));
    let tl = w.svc.timeline_offsets()[0];
    gpu_complete(&mut w, tl, head.seqno);
    poll(&mut w);
    assert_eq!(q(&mut w, 0), 5, "retirement signalled the point");
    assert_eq!(send_raw(&mut w, inc, Opcode::SyncobjTimelineWait, &wait[..n]).unwrap().errno, 0);
}

// ---- malformed input ------------------------------------------------------------------------------------------------------------

fn raw_frame(w: &mut W, bytes: &[u8]) -> Resp {
    let mut out = vec![0u8; 4096];
    match w.svc.handle(&mut hw!(w), bytes, &mut out).unwrap() {
        Handled::Reply(n) => parse(&out[..n]),
        Handled::Parked => panic!("parked"),
    }
}

#[test]
fn malformed_messages_are_refused_with_the_right_errno() {
    let mut w = world();
    let inc = open(&mut w);
    let hdr = |op: Opcode, len: usize| {
        let mut f = vec![0u8; HEADER_BYTES + len];
        Header::request(op, 9, inc, len as u32).encode(&mut f).unwrap();
        f
    };
    // header level: bad magic -> -EINVAL, a frame shorter than a header -> -EFAULT, a declared length that is not the real one -> -EFAULT
    let mut bad = hdr(Opcode::GemBusy, 8);
    bad[0] = 0;
    let r = raw_frame(&mut w, &bad);
    assert_eq!((r.status, r.errno), (Status::Malformed, -errno::EINVAL));
    let r = raw_frame(&mut w, &[1, 2, 3]);
    assert_eq!((r.status, r.errno), (Status::Malformed, -errno::EFAULT));
    let mut f = hdr(Opcode::GemBusy, 8);
    f.pop();
    assert_eq!(raw_frame(&mut w, &f).errno, -errno::EFAULT);
    // a REPLY sent as a request
    let mut f = hdr(Opcode::GemBusy, 8);
    f[12] = 1;
    assert_eq!((raw_frame(&mut w, &f).status, raw_frame(&mut w, &f).errno), (Status::Malformed, -errno::EINVAL));
    // payload level
    let mut p = hdr(Opcode::GetParam, 8);
    p[HEADER_BYTES + 4] = 1; // the reserved word
    let r = raw_frame(&mut w, &p);
    assert_eq!((r.status, r.errno), (Status::Malformed, -errno::EINVAL), "GETPARAM with a non-zero reserved word");
    let r = raw_frame(&mut w, &hdr(Opcode::GemCreate, 7)); // one byte short of a u64
    assert_eq!((r.status, r.errno), (Status::Malformed, -errno::EFAULT));
    let r = raw_frame(&mut w, &hdr(Opcode::GemCreate, 9));
    assert_eq!((r.status, r.errno), (Status::Malformed, -errno::EINVAL), "one byte too long: trailing bytes");
    let mut ext = [0u8; 32];
    GemCreateExtReq::encode(&mut ext, GemCreateExtHead { size: 4096, ..Default::default() }, &[]).unwrap();
    ext[12] = 65; // region_count past the validation limit
    let mut f = hdr(Opcode::GemCreateExt, 32);
    f[HEADER_BYTES..].copy_from_slice(&ext);
    let r = raw_frame(&mut w, &f);
    assert_eq!((r.status, r.errno), (Status::Malformed, -errno::ENOMEM));
    // a EXECBUFFER2 whose tables are cut short is Malformed; one that decodes but breaks a Linux rule is an ordinary -EINVAL
    let (batch, result) = (create(&mut w, inc, 4096), create(&mut w, inc, 4096));
    let good = exec_payload(result, batch, &[], None, 0);
    let r = send_raw(&mut w, inc, Opcode::Execbuffer2, &good[..good.len() - 8]).unwrap();
    assert_eq!((r.status, r.errno), (Status::Malformed, -errno::EFAULT));
    let mut zero = good.clone();
    zero[24..28].copy_from_slice(&0u32.to_le_bytes()); // buffer_count = 0: check_buffer_count, -EINVAL
    let r = send_raw(&mut w, inc, Opcode::Execbuffer2, &zero[..64]).unwrap();
    assert_eq!((r.status, r.errno), (Status::Ok, -errno::EINVAL));
    // an opcode the frame's minor predates
    let mut f = hdr(Opcode::GemBusy, 8);
    f[6] = 0;
    assert_eq!(raw_frame(&mut w, &f).errno, -errno::EINVAL);
    // a non-zero incarnation on SESSION-OPEN, an unknown capability bit, and a pad on FD-CLOSE
    let mut f = hdr(Opcode::SessionOpen, 8);
    assert_eq!(raw_frame(&mut w, &f).status, Status::Malformed);
    f[24..32].copy_from_slice(&0u64.to_le_bytes());
    f[HEADER_BYTES] = 0x80;
    assert_eq!(raw_frame(&mut w, &f).status, Status::Malformed);
    // the service is still sound after all of that
    assert_eq!(call(&mut w, inc, Opcode::GemBusy, &GemBusyReq { handle: batch, ..Default::default() }).errno, 0);
    assert_eq!(w.svc.live_sessions(), 1);
}

#[test]
fn getparam_and_query_answer_through_the_wire() {
    let mut w = world();
    let inc = open(&mut w);
    let id = call(&mut w, inc, Opcode::GetParam, &GetParamReq { param: 4, ..Default::default() }).body::<GetParamRep>().value;
    assert_eq!(id, 0x591c, "I915_PARAM_CHIPSET_ID");
    let r = call(&mut w, inc, Opcode::GetParam, &GetParamReq { param: 0x7777, ..Default::default() });
    assert_eq!((r.status, r.errno), (Status::Ok, -errno::EINVAL), "an unknown param is the ioctl's -EINVAL");
    // engine info: only the render engine is routed
    let mut q = [0u8; 128];
    let n = encode_query_request(&mut q, 0, &[QueryItem { query_id: 2, length: 0, ..Default::default() }]).unwrap();
    let r = send_raw(&mut w, inc, Opcode::Query, &q[..n]).unwrap();
    let (e, items) = i915_wire::msg::device::decode_query_reply(&r.payload).unwrap();
    assert_eq!(e, 0);
    assert!(items[0].unwrap().length > 0);
    // REG_READ: the render timestamp only
    let r = call(&mut w, inc, Opcode::RegRead, &RegReadReq { offset: 0x2358 | 1 });
    assert_eq!(r.errno, 0);
    assert!(call(&mut w, inc, Opcode::RegRead, &RegReadReq { offset: 0x2000 }).is_err(errno::EINVAL));
    assert!(call(&mut w, inc, Opcode::RegRead, &RegReadReq { offset: 0x2358 | 2 }).is_err(errno::EINVAL));
}

// ---- growth --------------------------------------------------------------------------------------------------------------------

#[test]
fn a_table_that_cannot_grow_refuses_by_name_before_the_thing_exists() {
    let mut w = world();
    let inc = open(&mut w);
    let live = w.svc.gem().object_count();
    w.svc.set_table_budget(Some(0));
    let r = call(&mut w, inc, Opcode::GemCreate, &GemCreateReq { size: 4096 });
    assert!(r.is_err(errno::ENOMEM), "errno {}", r.errno);
    assert!(w.log.0.iter().any(|l| l.contains("refused") && l.contains("OUT-OF-MEMORY") || l.contains("NOMEM")), "{:?}", w.log.0.last());
    assert_eq!(w.svc.gem().object_count(), live, "no object was created");
    let r = call(&mut w, 0, Opcode::SessionOpen, &SessionOpenReq::default());
    assert!(r.is_err(errno::ENOMEM));
    assert_eq!(w.svc.live_sessions(), 1, "no session was created");
    w.svc.set_table_budget(None);
    assert_eq!(call(&mut w, inc, Opcode::GemCreate, &GemCreateReq { size: 4096 }).errno, 0);
}

#[test]
fn sessions_are_a_growable_table() {
    let mut w = world();
    let mut incs = vec![];
    for _ in 0..200 {
        incs.push(open(&mut w));
    }
    assert_eq!(w.svc.live_sessions(), 200, "no session cap: 200 sessions live at once");
    for inc in incs {
        w.svc.session_dead(&mut hw!(w), inc);
    }
    assert_eq!(w.svc.live_sessions(), 0);
    assert_eq!(w.svc.gem().session_count(), 0);
    assert_eq!(w.svc.gem().vm_count(), 0);
    assert_eq!(w.svc.pool().pages_out(), 0);
}

#[test]
fn the_status_page_holds_a_bounded_number_of_timelines_and_says_so_by_name() {
    let mut w = world();
    let inc = open(&mut w);
    let (batch, result) = (create(&mut w, inc, 4096), create(&mut w, inc, 4096));
    let bl = lease(&mut w, inc, batch, 4096);
    write_batch(&mut w, &bl);
    let mut refused_at = None;
    for i in 0..70 {
        let ctx = call(&mut w, inc, Opcode::ContextCreate, &ContextCreateReq {}).body::<ContextRep>().ctx_id;
        let (r, head) = submit(&mut w, inc, &exec_payload(result, batch, &[], None, ctx));
        if head.is_none() {
            assert!(r.is_err(errno::ENOMEM), "errno {}", r.errno);
            refused_at = Some(i);
            break;
        }
    }
    let n = refused_at.expect("the status page ran out of timeline slots");
    assert!(refused(&w, "HWSP-TIMELINE-SLOTS-EXHAUSTED") || refused_by_engine(&w), "named: {:?}", w.log.0.last());
    assert!(n >= 30, "tens of contexts fit one status page, got {n}");
}

fn refused_by_engine(w: &W) -> bool {
    w.log.0.iter().any(|l| l.contains("refused") && (l.contains("OUT-OF-MEMORY") || l.contains("CONTEXT") || l.contains("TAG")))
}

// ---- hang ------------------------------------------------------------------------------------------------------------------------

#[test]
fn a_hung_request_fails_its_fence_with_eio_charges_the_context_and_the_engine_recovers() {
    let mut w = world();
    let inc = open(&mut w);
    let (batch, result) = (create(&mut w, inc, 4096), create(&mut w, inc, 4096));
    let bl = lease(&mut w, inc, batch, 4096);
    write_batch(&mut w, &bl);
    let out = syncobj(&mut w, inc, 0);
    let head = submit(&mut w, inc, &exec_payload(result, batch, &[(out, FENCE_SIGNAL)], None, 0)).1.unwrap();
    let sf = call(&mut w, inc, Opcode::SyncobjHandleToFd, &SyncobjHandleToFdReq { handle: out, flags: sync::flags::HANDLE_TO_FD_EXPORT_SYNC_FILE, point: 0 }).body::<SyncobjTokenRep>().token as u32;
    // the hardware accepts the submission (CSB promote) and never advances the timeline
    let hwsp = *w.gm.alloc.grants().find(|g| g.kind == ObjKind::Hwsp).unwrap();
    w.mem.put(hwsp.cpu_mapping + 0x10 * 4, 1);
    w.mem.put(hwsp.cpu_mapping + 0x11 * 4, 0);
    w.mem.put(hwsp.cpu_mapping + 0x1f * 4, 0);
    poll(&mut w);
    assert_eq!(w.svc.requests_in_flight(), 1, "not hung yet");
    w.now += 5_000; // far past the 1000 ms stall bound
    w.svc.poll(&mut hw!(w));
    assert!(w.log.has("[I915-UAPI HANG name=ENGINE-HUNG"), "{:?}", w.log.0.iter().rev().take(4).collect::<Vec<_>>());
    assert_eq!(w.svc.requests_in_flight(), 0, "the lost request is no longer in flight");
    let r = call(&mut w, inc, Opcode::SyncFileWait, &SyncFileWaitReq { token: sf, timeout_ns: 0, ..Default::default() });
    assert_eq!(r.body::<SyncFileWaitRep>().fence_error, -errno::EIO, "the out fence carries the failure");
    let stats = call(&mut w, inc, Opcode::GetResetStats, &GetResetStatsReq::default()).body::<GetResetStatsRep>();
    assert_eq!(stats.batch_active, 1, "the context that hung is charged");
    assert_eq!(w.svc.rig().epoch, 1, "the engine was reset");
    let _ = head;
}
