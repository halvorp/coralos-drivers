// SPDX-License-Identifier: GPL-2.0-only
//! The live loop (`live::Live`) over a FAKE endpoint pair and the simulated GPU: whole sessions driven as frames, large calls through a
//! payload region, peer death mid-EXECBUFFER, and - the doctrine test - what the loop is allowed to wake for.
#![cfg(feature = "p3-exec")]
mod exec_common;
use coralos_hal::fake::FakeDma;
use exec_common::*;
use i915_device::exec::dma::ObjKind;
use i915_device::live::{ConnId, Live, LiveStats, PayloadRegions, RegionError, Step, Wake, REPLY_RETRY_MS};
use i915_device::uapi::{bring_up_render, Hw, RenderConfig, COMPLETION_POLL_MS};
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_gt_core::emit::FlushQuirks;
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;
use i915_wire::msg::device::*;
use i915_wire::msg::exec::{self, ExecObject, ExecReply};
use i915_wire::msg::gem::*;
use i915_wire::msg::session::*;
use i915_wire::msg::Body;
use i915_wire::region::{RegionRef, MAX_REGION_PAYLOAD};
use i915_wire::{errno, Header, Opcode, Status, FLAG_PAYLOAD_REGION, HEADER_BYTES, MAX_PAYLOAD};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

const GIB: u64 = 1 << 30;
const STALL_MS: u64 = 1000;
const BATCH_VA: u64 = 0x1000_0000_0000;
const RESULT_VA: u64 = 0x2000_0000;
const MI_STORE_DWORD_IMM_GEN4: u32 = (0x20 << 23) | 2; // intel_gpu_commands.h:136
const MI_BATCH_BUFFER_END: u32 = 0x0a << 23; // intel_gpu_commands.h:62

mod net_common;
use net_common::*;

#[derive(Default)]
struct R {
    regions: BTreeMap<u32, Vec<u8>>,
    reads: Vec<(u32, u64, usize)>,
}

#[derive(Clone, Default)]
struct FakeRegions(Rc<RefCell<R>>);

impl PayloadRegions for FakeRegions {
    fn read(&mut self, _: u16, region: u32, offset: u64, out: &mut [u8]) -> Result<(), RegionError> {
        let mut r = self.0.borrow_mut();
        r.reads.push((region, offset, out.len()));
        let m = r.regions.get(&region).ok_or(RegionError::Unknown { region })?;
        let end = offset as usize + out.len();
        if end > m.len() {
            return Err(RegionError::OutOfRange { region, offset, len: out.len() as u64, size: m.len() as u64 });
        }
        out.copy_from_slice(&m[offset as usize..end]);
        Ok(())
    }
    fn write(&mut self, _: u16, region: u32, offset: u64, data: &[u8]) -> Result<(), RegionError> {
        let mut r = self.0.borrow_mut();
        let m = r.regions.get_mut(&region).ok_or(RegionError::Unknown { region })?;
        let end = offset as usize + data.len();
        if end > m.len() {
            return Err(RegionError::OutOfRange { region, offset, len: data.len() as u64, size: m.len() as u64 });
        }
        m[offset as usize..end].copy_from_slice(data);
        Ok(())
    }
}

// ---- the world -----------------------------------------------------------------------------------------------------------------

type L = Live<FakeT, FakeRegions, Chip, FakeDma, Mem, Lines>;

struct W {
    live: L,
    t: FakeT,
    regions: FakeRegions,
    leases: FakeLeases,
    mem: Mem,
    /// The client every helper talks as.
    cid: ConnId,
    rid: u64,
    csb: u32,
}

fn world() -> W {
    let (chip, mem, ex, mut gm) = built();
    let mut ex = ex;
    let mut log = Lines::default();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    let dev = UapiDevice::new(kbl(), 0x09, sseu, hz, 8 * GIB).unwrap();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    let cfg = RenderConfig { quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, stall_bound_ms: STALL_MS, keep: None };
    let mut svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, cfg).unwrap();
    let leases = FakeLeases::default();
    svc.set_lease_sys(Box::new(leases.clone()));
    let t = FakeT::default();
    t.0.borrow_mut().now = 1;
    let regions = FakeRegions::default();
    let mut live = Live::new(t.clone(), regions.clone(), svc, ex, gm, log).unwrap();
    let _ = chip;
    // a client connects and the loop accepts it; the counters then start from zero so each test counts only its own wakes
    let cid = t.connect();
    t.0.borrow_mut().default = Some(cid);
    t.0.borrow_mut().script.push_back(Ev::Wake(Wake::Accept));
    assert_eq!(live.step(), Step::Woke(Wake::Accept));
    assert_eq!(live.stats.conns_accepted, 1);
    live.stats = LiveStats::default();
    W { live, t, regions, leases, mem, cid, rid: 0, csb: 0 }
}

fn region_frame(op: Opcode, id: u64, inc: u64, rr: &RegionRef) -> Vec<u8> {
    let mut f = vec![0u8; HEADER_BYTES + i915_wire::region::REGION_REF_BYTES];
    rr.encode(&mut f[HEADER_BYTES..]).unwrap();
    let mut h = Header::request(op, id, inc, i915_wire::region::REGION_REF_BYTES as u32);
    h.flags = FLAG_PAYLOAD_REGION;
    h.encode(&mut f).unwrap();
    f
}

impl W {
    fn sent(&self) -> usize {
        self.t.messages_sent(self.cid)
    }
    fn last_reply(&self) -> Resp {
        parse(self.t.0.borrow().clients[&self.cid].sent.last().expect("a reply was sent"))
    }
    /// The client sends one request frame (as the messages it travels as) and the loop runs through exactly one wake per message.
    fn deliver(&mut self, f: Vec<u8>) -> Step {
        let msgs: Vec<Vec<u8>> = match i915_wire::frag::split(&f, i915_wire::frag::Limits::KERNEL) {
            Ok(c) => c.map(|m| m.to_vec()).collect(),
            Err(_) => vec![f],
        };
        let mut last = Step::Closed;
        for m in msgs {
            self.t.0.borrow_mut().script.push_back(Ev::To(self.cid, m));
            last = self.live.step();
        }
        last
    }
    fn wake(&mut self, w: Wake) -> Step {
        self.t.0.borrow_mut().script.push_back(Ev::Wake(w));
        self.live.step()
    }
    fn raw(&mut self, inc: u64, op: Opcode, payload: &[u8]) -> Option<Resp> {
        self.rid += 1;
        let before = self.sent();
        self.deliver(frame(op, self.rid, inc, payload));
        (self.sent() > before).then(|| {
            let r = self.last_reply();
            assert_eq!((r.hdr.opcode, r.hdr.request_id), (op, self.rid), "the reply echoes the request");
            r
        })
    }
    fn call<B: Body>(&mut self, inc: u64, op: Opcode, req: &B) -> Resp {
        let mut p = vec![0u8; 4096];
        let n = req.to_payload(&mut p).unwrap();
        self.raw(inc, op, &p[..n]).expect("the call parked")
    }
    fn open(&mut self) -> u64 {
        let r = self.call(0, Opcode::SessionOpen, &SessionOpenReq::default());
        r.body::<SessionOpenRep>().incarnation
    }
    fn create(&mut self, inc: u64, size: u64) -> u32 {
        self.call(inc, Opcode::GemCreate, &GemCreateReq { size }).body::<GemCreateRep>().handle
    }
    fn lease(&mut self, inc: u64, handle: u32, len: u64) -> GemMmapRep {
        let off = self.call(inc, Opcode::GemMmapOffset, &GemMmapOffsetReq { handle, flags: 2, ..Default::default() }).body::<GemMmapOffsetRep>();
        self.call(inc, Opcode::GemMmapOpen, &GemMmapOpenReq { offset: off.offset, len }).body::<GemMmapRep>()
    }
    fn write_batch(&mut self, l: &GemMmapRep) {
        let va = RESULT_VA;
        for (i, d) in [MI_STORE_DWORD_IMM_GEN4, va as u32, (va >> 32) as u32, 0xc0ff_ee42, MI_BATCH_BUFFER_END].iter().enumerate() {
            self.live.gm.store32(l.grant_addr + 4 * i as u64, *d).unwrap();
        }
    }
    /// The hardware completes the oldest submission: the CSB promotes then retires, the context's own timeline page reaches `seqno`.
    fn gpu_complete(&mut self, timeline_dev: u64, seqno: u32) {
        let hwsp = *self.live.gm.alloc.grants().find(|g| g.kind == ObjKind::Hwsp).unwrap();
        let first = self.csb;
        let entry = |i: u32| hwsp.cpu_mapping + (0x10 + 2 * (i % 6)) as u64 * 4;
        self.mem.put(entry(first), 1);
        self.mem.put(entry(first) + 4, 0);
        self.mem.put(entry(first + 1), 1 << 3);
        self.mem.put(entry(first + 1) + 4, 0);
        let tl_cpu = self.live.gm.alloc.cpu_of(timeline_dev, 4).unwrap();
        self.mem.put(tl_cpu, seqno);
        self.mem.put(hwsp.cpu_mapping + 0x1f * 4, (first + 1) % 6);
        self.csb += 2;
    }
    fn waits(&self) -> Vec<(Option<u64>, bool)> {
        self.t.0.borrow().waits.clone()
    }
    fn snapshot(&self) -> [usize; 10] {
        let s = &self.live.svc;
        [s.live_sessions(), s.gem().session_count(), s.gem().object_count(), s.live_leases(), s.live_hw_contexts(), s.live_vm_records(), s.live_object_pages(), s.requests_in_flight(), s.pool().pages_out(), s.live_kernel_leases()]
    }
}

trait ToVec: Body {
    fn to_payload_vec(&self) -> Vec<u8> {
        let mut v = vec![0u8; Self::SIZE];
        self.to_payload(&mut v).unwrap();
        v
    }
}
impl<T: Body> ToVec for T {}

const PINNED: u64 = 1 << 4;
const SUPPORTS_48B: u64 = 1 << 3;
const WRITE: u64 = 1 << 2;

fn exec_payload(result: u32, batch: u32, ctx: u32) -> Vec<u8> {
    let objs = [
        ExecObject { handle: result, offset: RESULT_VA, flags: PINNED | SUPPORTS_48B | WRITE, ..Default::default() },
        ExecObject { handle: batch, offset: BATCH_VA, flags: PINNED | SUPPORTS_48B, ..Default::default() },
    ];
    let hdr = exec::ExecHeader { rsvd1: ctx as u64, batch_len: 24, flags: exec::flags::NO_RELOC, ..Default::default() };
    let mut out = vec![0u8; 4096];
    let n = exec::encode_request(&mut out, hdr, &objs, &[], &[], &[]).unwrap();
    out.truncate(n);
    out
}

/// A session with one batch+result pair and one submission in flight; returns `(inc, timeline dword, seqno)`.
fn start_work(w: &mut W) -> (u64, u64, u32) {
    let inc = w.open();
    let (batch, result) = (w.create(inc, 4096), w.create(inc, 4096));
    let bl = w.lease(inc, batch, 4096);
    w.write_batch(&bl);
    let r = w.raw(inc, Opcode::Execbuffer2, &exec_payload(result, batch, 0)).unwrap();
    let head = ExecReply::decode(&r.payload).unwrap().head;
    (inc, w.live.svc.timeline_addrs()[0], head.seqno)
}

// ---- tests ---------------------------------------------------------------------------------------------------------------------

#[test]
fn a_whole_session_runs_over_the_live_loop_one_wake_per_request() {
    let mut w = world();
    let base = w.snapshot();
    let inc = w.open();
    assert_eq!(w.last_reply().hdr.session_incarnation, inc, "the reply header carries the new incarnation");
    let (batch, result) = (w.create(inc, 4096), w.create(inc, 4096));
    let bl = w.lease(inc, batch, 4096);
    w.write_batch(&bl);
    let q = w.call(inc, Opcode::GetParam, &GetParamReq { param: 1, ..Default::default() });
    assert_eq!(q.status, Status::Ok);
    let r = w.raw(inc, Opcode::Execbuffer2, &exec_payload(result, batch, 0)).unwrap();
    let head = ExecReply::decode(&r.payload).expect("submitted").head;
    assert_eq!(w.live.svc.requests_in_flight(), 1);
    // the GPU finishes; the loop notices at its next wake (a deadline), the request retires, the fence signals
    let tl = w.live.svc.timeline_addrs()[0];
    w.gpu_complete(tl, head.seqno);
    w.wake(Wake::Deadline);
    assert_eq!(w.live.svc.requests_in_flight(), 0);
    assert_eq!(w.raw(inc, Opcode::SessionClose, &[]).unwrap().errno, 0);
    w.wake(Wake::Deadline);
    assert_eq!(w.snapshot(), base, "everything the session held is gone");
    assert_eq!(w.live.stats.calls, 8);
    assert_eq!(w.live.stats.request_wakes, 8, "one wake per request");
    assert_eq!(w.live.stats.refused, 0);
    assert_eq!(w.live.stats.waits, 10, "10 waits: 8 requests + 2 deadlines, nothing else");
}

#[test]
fn the_largest_fragmentable_frame_is_served_and_bigger_or_broken_ones_are_refused_by_name() {
    let mut w = world();
    // the most that travels as messages: 15 full messages = 61440 bytes, a HELLO that declares 61400 payload bytes
    let hello = frame(Opcode::Hello, 1, 0, &vec![0u8; 61440 - HEADER_BYTES]);
    assert_eq!(hello.len(), 61440);
    w.deliver(hello);
    assert_eq!(w.t.0.borrow().recv_buf, 4096, "the buffer the loop receives into is ONE message, not a whole frame and not 64 bytes");
    assert_eq!(w.live.stats.request_wakes, 15, "one wake per message of the frame");
    let r = w.last_reply();
    assert_eq!((r.status, r.errno), (Status::Ok, 0), "the full-size frame was served");
    assert_eq!(w.live.stats.calls, 1, "once, only when the whole frame was in");
    assert_eq!(w.live.stats.refused, 0);

    // a header that needs a 16th message: refused by name at the FIRST message (nothing is kept), and the request is answered
    let over = frame(Opcode::Hello, 2, 0, &vec![0u8; 61441 - HEADER_BYTES]);
    w.t.0.borrow_mut().script.push_back(Ev::To(w.cid, over[..4096].to_vec()));
    w.live.step();
    let r = w.last_reply();
    assert_eq!(r.status, Status::Malformed);
    assert_eq!(r.errno, -errno::ENOMEM);
    assert!(w.live.log.has("frame refused name=FRAG-TOO-MANY-MESSAGES"), "{:?}", w.live.log.0.last());
    assert_eq!(w.live.stats.refused, 1);

    // a header that declares more than the wire's own payload bound is refused with the wire's name
    let mut h = vec![0u8; HEADER_BYTES];
    Header::request(Opcode::Hello, 3, 0, 0).encode(&mut h).unwrap();
    h[32..36].copy_from_slice(&(MAX_PAYLOAD + 1).to_le_bytes());
    w.deliver(h);
    assert_eq!(w.last_reply().status, Status::Malformed);
    assert!(w.live.log.has("frame refused name=WIRE-PAYLOAD-TOO-LARGE"));

    // a message longer than the kernel can deliver is reported oversized by the receive path and answered by name
    w.t.0.borrow_mut().script.push_back(Ev::To(w.cid, vec![0u8; 4097]));
    w.live.step();
    assert_eq!(w.last_reply().status, Status::Malformed);
    assert!(w.live.log.has("frame refused name=WIRE-PAYLOAD-TOO-LARGE frame_bytes=4097"), "{:?}", w.live.log.0.last());

    // and the first generation's four-byte protocol still answers
    w.deliver(vec![b'I', 1, 0]);
    assert_eq!(w.t.0.borrow().clients[&w.cid].sent.last().unwrap(), &vec![b'i', 1, 0, 0]);
}

/// A 1000-object / 1024-relocation EXECBUFFER2 payload: far larger than one inline frame.
fn big_exec_payload() -> Vec<u8> {
    let mut objs: Vec<ExecObject> = (0..1000).map(|i| ExecObject { handle: 7000 + i, offset: 0x4000_0000 + i as u64 * 0x1000, flags: PINNED | SUPPORTS_48B, ..Default::default() }).collect();
    objs[0].relocation_count = 1024; // the relocation table belongs to the first object (reloc_total must equal the sum)
    let relocs: Vec<exec::Reloc> = (0..1024).map(|_| exec::Reloc::default()).collect();
    let hdr = exec::ExecHeader { batch_len: 24, flags: exec::flags::NO_RELOC, ..Default::default() };
    let mut out = vec![0u8; 200_000];
    let n = exec::encode_request(&mut out, hdr, &objs, &relocs, &[], &[]).unwrap();
    out.truncate(n);
    out
}

#[test]
fn a_call_larger_than_a_frame_travels_through_a_payload_region() {
    let mut w = world();
    let inc = w.open();
    let big = big_exec_payload();
    assert!(big.len() > MAX_PAYLOAD as usize && big.len() as u32 <= MAX_REGION_PAYLOAD, "this payload cannot be inline: {}", big.len());
    // the client's region: the payload sits at offset 4096, with a reply area behind it
    let mut region = vec![0xEEu8; 4096];
    region.extend_from_slice(&big);
    region.resize(region.len() + 8192, 0);
    w.regions.0.borrow_mut().regions.insert(9, region);
    let rr = RegionRef { region: 9, offset: 4096, len: big.len() as u32, reply_cap: 8192, reply_offset: (4096 + big.len()) as u64, reply_region: 9 };
    w.rid += 1;
    let id = w.rid;
    w.deliver(region_frame(Opcode::Execbuffer2, id, inc, &rr));
    assert_eq!(w.regions.0.borrow().reads, vec![(9, 4096, big.len())], "the device copied exactly the named bytes, once");
    let r = w.last_reply();
    assert_eq!(r.hdr.request_id, id);
    // the service saw the WHOLE payload: it decoded (not Malformed/Truncated) and refused on the first unknown object handle
    assert_eq!(r.status, Status::Ok, "decoded in full; log {:?}", w.live.log.0.iter().rev().take(3).collect::<Vec<_>>());
    assert_ne!(r.errno, 0, "and refused the bogus handles");
    assert!(w.live.log.has("op=EXECBUFFER2 refused"), "{:?}", w.live.log.0.last());
    assert!(!r.hdr.is_region(), "a small reply to a region call is an inline reply");
    assert_eq!((w.live.stats.region_calls, w.live.stats.region_replies), (1, 0));
    // the same bytes any shorter would have been Truncated: prove the length matters
    let short = RegionRef { len: big.len() as u32 - 10, ..rr };
    w.deliver(region_frame(Opcode::Execbuffer2, id + 1, inc, &short));
    assert!(w.last_reply().status == Status::Malformed, "a payload cut short is malformed, by the wire's own decode");
}

#[test]
fn a_reply_that_does_not_fit_a_frame_goes_to_the_clients_reply_area_and_is_refused_by_name_without_one() {
    let mut w = world();
    let inc = w.open();
    // shrink the inline reply limit so an ordinary reply (GETPARAM's) "does not fit": the path the 64 KiB limit takes for a long QUERY
    w.live.inline_reply_max = 8;
    let mut p = vec![0u8; 64];
    let n = GetParamReq { param: 1, ..Default::default() }.to_payload(&mut p).unwrap();
    w.regions.0.borrow_mut().regions.insert(3, [p[..n].to_vec(), vec![0u8; 512]].concat());
    let rr = RegionRef { region: 3, offset: 0, len: n as u32, reply_cap: 512, reply_offset: n as u64, reply_region: 3 };
    w.deliver(region_frame(Opcode::GetParam, 50, inc, &rr));
    let r = w.last_reply_header_only();
    assert!(r.0.is_region() && r.0.is_reply(), "the reply says it is in the region");
    let placed = RegionRef::decode(&r.1).unwrap();
    assert_eq!((placed.region, placed.offset), (3, n as u64));
    let area = w.regions.0.borrow().regions[&3][n..n + placed.len as usize].to_vec();
    let rep = i915_wire::codec::Reader::new(&area).u32("status").unwrap();
    assert_eq!(rep, Status::Ok as u32, "the reply payload (status, errno, body) sits in the area");
    assert!(placed.len as usize > 8);
    assert_eq!(w.live.stats.region_replies, 1);
    // no reply area (or one too small): refused BY NAME, never truncated
    let rr0 = RegionRef { reply_cap: 4, ..rr };
    w.deliver(region_frame(Opcode::GetParam, 51, inc, &rr0));
    let r = w.last_reply();
    assert!(r.is_err(errno::EFAULT));
    assert!(w.live.log.has("name=REPLY-TOO-LARGE"));
}

impl W {
    /// A new client connects (the old one is gone) and the loop accepts it.
    fn reconnect(&mut self) {
        self.cid = self.t.connect();
        self.t.0.borrow_mut().default = Some(self.cid);
        self.wake(Wake::Accept);
    }
    fn last_reply_header_only(&self) -> (Header, Vec<u8>) {
        let s = self.t.0.borrow();
        let (h, p) = Header::decode(s.clients[&self.cid].sent.last().unwrap()).unwrap();
        (h, p.to_vec())
    }
}

#[test]
fn a_region_that_cannot_be_reached_is_refused_by_name_and_the_production_binding_says_unavailable() {
    use i915_device::live::NoRegions;
    let mut w = world();
    let inc = w.open();
    let rr = RegionRef { region: 77, offset: 0, len: 64, reply_cap: 0, reply_offset: 0, reply_region: 77 };
    // unknown region
    w.deliver(region_frame(Opcode::GetParam, 1, inc, &rr));
    assert!(w.last_reply().is_err(errno::EINVAL));
    assert!(w.live.log.has("name=PAYLOAD-REGION-UNKNOWN"));
    // out of range
    w.regions.0.borrow_mut().regions.insert(77, vec![0u8; 32]);
    w.deliver(region_frame(Opcode::GetParam, 2, inc, &rr));
    assert!(w.last_reply().is_err(errno::EFAULT));
    assert!(w.live.log.has("name=PAYLOAD-REGION-OUT-OF-RANGE"));
    // a bad layout never reaches the host
    let mut f = region_frame(Opcode::GetParam, 3, inc, &rr);
    f[HEADER_BYTES + 4] = 9;
    let reads = w.regions.0.borrow().reads.len();
    w.deliver(f);
    assert_eq!(w.last_reply().status, Status::Malformed);
    assert_eq!(w.regions.0.borrow().reads.len(), reads, "no read was attempted");
    // the production binding
    let mut un = NoRegions;
    assert_eq!(un.read(1, 1, 0, &mut [0u8; 4]).unwrap_err().name(), "PAYLOAD-REGION-UNAVAILABLE");
    assert_eq!(un.write(1, 1, 0, &[0u8; 4]).unwrap_err().name(), "PAYLOAD-REGION-UNAVAILABLE");
}

#[test]
fn a_peer_that_dies_mid_execbuffer_frees_everything_once_the_work_retires() {
    let mut w = world();
    let base = w.snapshot();
    let (inc, tl, seqno) = start_work(&mut w);
    assert_eq!(w.live.svc.requests_in_flight(), 1);
    assert!(w.snapshot() != base);
    // the peer dies with the request on the engine
    w.t.kill(w.cid);
    w.wake(Wake::Request(w.cid));
    assert_eq!(w.live.svc.live_sessions(), 0, "the session is dead the moment the peer is");
    assert_eq!(w.live.stats.sessions_freed_by_peer_death, 1);
    assert_eq!(w.live.svc.requests_in_flight(), 1, "the request is still on the engine");
    assert!(w.live.svc.gem().object_count() >= 2, "its objects live until the GPU is done with them");
    let _ = inc;
    // the GPU finishes; the next wake retires the request and frees what it held
    w.gpu_complete(tl, seqno);
    w.wake(Wake::Deadline);
    assert_eq!(w.snapshot(), base, "nothing is left: objects, contexts, address space, pages");
    // and the same on a SEND that reports the peer dead
    w.reconnect();
    let (_inc2, tl2, seqno2) = start_work(&mut w);
    w.t.0.borrow_mut().clients.get_mut(&w.cid).unwrap().send_dead = true;
    w.rid += 1;
    let rid = w.rid;
    w.deliver(frame(Opcode::GemCreate, rid, _inc2, &GemCreateReq { size: 4096 }.to_payload_vec()));
    assert_eq!(w.live.svc.live_sessions(), 0, "a reply that finds the peer dead frees its sessions too");
    w.gpu_complete(tl2, seqno2);
    w.wake(Wake::Deadline);
    assert_eq!(w.snapshot(), base);
}

#[test]
fn the_loop_blocks_without_a_deadline_when_idle_and_wakes_only_for_what_it_asked() {
    let mut w = world();
    // idle, nothing ever happened: no deadline at all
    assert_eq!(w.live.next_deadline(), None);
    let inc = w.open();
    let (batch, result) = (w.create(inc, 4096), w.create(inc, 4096));
    let bl = w.lease(inc, batch, 4096);
    w.write_batch(&bl);
    for (d, accept) in w.waits() {
        assert_eq!((d, accept), (None, true), "every wait of an idle device is unbounded: it sleeps until a request");
    }
    // work in flight, no engine interrupt: a completion poll and the stall check, and nothing past them
    let now = w.t.0.borrow().now;
    let r = w.raw(inc, Opcode::Execbuffer2, &exec_payload(result, batch, 0)).unwrap();
    let head = ExecReply::decode(&r.payload).unwrap().head;
    assert_eq!(w.live.next_deadline(), Some(now + COMPLETION_POLL_MS), "a timer, because work is in flight and no interrupt is armed");
    // with the engine interrupt armed the poll is gone: only the stall bound remains, exactly `stall_bound` after the watch armed
    w.t.0.borrow_mut().irq = true;
    let d = w.live.next_deadline().expect("the stall check");
    assert!(d >= now + STALL_MS - 1 && d <= now + STALL_MS + COMPLETION_POLL_MS + 2, "stall deadline {d} vs now {now}");
    w.t.0.borrow_mut().irq = false;
    // a parked timed wait adds its own deadline
    let wait = GemWaitReq { handle: result, flags: 0, timeout_ns: 5_000_000 };
    assert!(w.raw(inc, Opcode::GemWait, &wait.to_payload_vec()).is_none(), "the wait parks");
    assert_eq!(w.live.svc.parked_calls(), 1);
    let nd = w.live.next_deadline().unwrap();
    assert!(nd <= now + COMPLETION_POLL_MS, "{nd}");
    // the timer fires at the poll time: nothing completes, the wait is not due yet, the loop asks again - one wait per wake
    let before = w.waits().len();
    let t1 = now + COMPLETION_POLL_MS;
    w.t.0.borrow_mut().script.push_back(Ev::At(Wake::Deadline, t1));
    w.live.step();
    assert_eq!(w.waits().len(), before + 1, "exactly one wait per wake");
    // the GPU finishes: the next wake retires it and the deadline vanishes again
    let tl = w.live.svc.timeline_addrs()[0];
    w.gpu_complete(tl, head.seqno);
    w.t.0.borrow_mut().script.push_back(Ev::At(Wake::Deadline, t1 + 1));
    w.live.step();
    assert_eq!(w.live.svc.requests_in_flight(), 0);
    let ready = w.sent();
    assert!(ready >= 1);
    assert_eq!(w.live.svc.parked_calls(), 0, "the parked wait was answered by the completion");
    assert_eq!(w.live.next_deadline(), None, "idle again: no timer, no poll");
    // counting wake-ups: with a script of N events the loop slept exactly N times, then found the transport closed
    let waits_so_far = w.live.stats.waits;
    let n = 5;
    for _ in 0..n {
        w.t.0.borrow_mut().script.push_back(Ev::Req(frame(Opcode::Hello, 1, 0, &[])));
    }
    while w.live.step() != Step::Closed {}
    assert_eq!(w.live.stats.waits, waits_so_far + n + 1, "one sleep per event, plus the one that saw the close - never a spin");
    assert_eq!(w.live.stats.request_wakes + w.live.stats.interrupt_wakes + w.live.stats.deadline_wakes, w.live.stats.waits - 1, "every sleep ended in a named wake");
}

#[test]
fn an_interrupt_wakes_the_loop_and_a_deadline_with_nothing_due_changes_nothing() {
    let mut w = world();
    let (inc, tl, seqno) = start_work(&mut w);
    w.t.0.borrow_mut().irq = true;
    w.gpu_complete(tl, seqno);
    assert_eq!(w.live.svc.requests_in_flight(), 1, "nothing noticed yet");
    assert_eq!(w.wake(Wake::Interrupt), Step::Woke(Wake::Interrupt));
    assert_eq!(w.live.svc.requests_in_flight(), 0, "the interrupt wake advanced the engine");
    assert_eq!(w.live.stats.interrupt_wakes, 1);
    let snap = w.snapshot();
    w.wake(Wake::Deadline);
    assert_eq!(w.snapshot(), snap, "a spurious deadline changes nothing");
    let _ = inc;
}

#[test]
fn a_full_reply_ring_holds_the_reply_and_the_requests_until_it_drains_and_keeps_order() {
    let mut w = world();
    let inc = w.open();
    let sent0 = w.sent();
    // the ring refuses the next two sends
    w.t.0.borrow_mut().clients.get_mut(&w.cid).unwrap().ring_full = 3; // the delivery's own send, the same step's retry, and one timer retry all find the ring full
    w.rid += 1;
    let id1 = w.rid;
    w.deliver(frame(Opcode::GemCreate, id1, inc, &GemCreateReq { size: 4096 }.to_payload_vec()));
    assert_eq!(w.sent(), sent0, "the reply is not out");
    assert!(w.live.next_deadline().is_some(), "an unsent reply is work in flight: a retry timer exists");
    assert_eq!(w.live.next_deadline(), Some(w.t.0.borrow().now + REPLY_RETRY_MS));
    // while it is unsent, no new request is taken
    let n_waits = w.waits().len();
    w.wake(Wake::Deadline); // the retry: the ring refuses once more
    assert_eq!(w.sent(), sent0);
    assert_eq!(w.waits()[n_waits].0, Some(w.t.0.borrow().now + REPLY_RETRY_MS));
    assert_eq!(w.t.0.borrow().held_at_wait[n_waits], vec![w.cid], "the wait takes no request from a connection whose reply is unsent");
    w.wake(Wake::Deadline); // the ring has room now
    assert_eq!(w.sent(), sent0 + 1, "delivered");
    assert_eq!(w.last_reply().hdr.request_id, id1);
    assert_eq!(w.live.stats.replies_dropped, 0);
    assert_eq!(w.live.next_deadline(), None, "and the timer is gone again");
}

// ---- GEM CPU mapping through the kernel's DMA lease -----------------------------------------------------------------------------

impl W {
    fn close_handle(&mut self, inc: u64, handle: u32) {
        assert_eq!(self.call(inc, Opcode::GemClose, &GemCloseReq { handle, ..Default::default() }).errno, 0);
    }
    /// A session with a mapped batch + an unmapped result, a submission in flight, and every handle closed and the mapping unmapped: the
    /// client is done with it all while the GPU is not. Returns `(inc, timeline dword, seqno, kernel lease handle)`.
    fn closed_while_in_flight(&mut self) -> (u64, u64, u32, u64) {
        let inc = self.open();
        let (batch, result) = (self.create(inc, 4096), self.create(inc, 4096));
        let bl = self.lease(inc, batch, 4096);
        self.write_batch(&bl);
        let r = self.raw(inc, Opcode::Execbuffer2, &exec_payload(result, batch, 0)).unwrap();
        let head = ExecReply::decode(&r.payload).unwrap().head;
        let tl = self.live.svc.timeline_addrs()[0];
        self.close_handle(inc, batch);
        self.close_handle(inc, result);
        assert_eq!(self.call(inc, Opcode::GemMmapClose, &GemMmapCloseReq { lease: bl.lease, ..Default::default() }).errno, 0);
        (inc, tl, head.seqno, bl.region_cap)
    }
}

#[test]
fn a_mapping_is_a_kernel_lease_over_the_object_with_the_pages_own_cache_type_for_this_sessions_client() {
    let mut w = world();
    let inc = w.open();
    let h = w.create(inc, 3 * 4096);
    let l = w.lease(inc, h, 3 * 4096);
    let k = w.leases.0.borrow();
    assert_eq!(k.created.len(), 1);
    let s = k.created[0];
    assert_eq!((s.length, s.offset), (3 * 4096, 0), "the object's whole run, whole pages");
    assert_eq!(s.coherence, coralos_hal::Coherence::NonSnooped);
    assert_eq!(s.peer, i915_device::conn::Peer { req_ep: 100 + w.cid.0 as u16, node: 0x7000 + w.cid.0 }, "the recipient is the client of THIS connection, as its connect announce named it");
    assert_eq!((l.cache, l.region_cap, l.region_offset, l.len), (i915_wire::msg::gem::cache::WRITE_COMBINING, k.live[0].0, 0, 3 * 4096));
    drop(k);
    // a second mapping of the same object by the same session shares the lease (kernel leases are page-exclusive)
    let l2 = w.lease(inc, h, 4096);
    assert_eq!(l2.region_cap, l.region_cap);
    assert_eq!(w.leases.live_count(), 1);
    assert_eq!(w.live.svc.live_kernel_leases(), 1);
}

#[test]
fn a_kernel_lease_outlives_gem_close_and_unmap_and_ends_only_after_the_gpu_retires_the_object() {
    let mut w = world();
    let base = w.snapshot();
    let (inc, tl, seqno, handle) = w.closed_while_in_flight();
    let free0 = w.live.svc.pool().pages_free();
    assert_eq!(w.leases.events(), vec![format!("create {handle:#x}")]);
    assert_eq!(w.live.svc.requests_in_flight(), 1);
    assert_eq!(w.leases.live_count(), 1, "GEM_CLOSE and the unmap do not end it: the GPU still uses the pages");
    assert_eq!(w.live.svc.pages_quarantined(), 0);
    // the GPU retires the request: the object is freed, the lease ends FIRST, then the pages go back to the pool
    w.gpu_complete(tl, seqno);
    w.wake(Wake::Deadline);
    assert_eq!(w.leases.events(), vec![format!("create {handle:#x}"), format!("revoke {handle:#x}")]);
    assert_eq!(w.leases.live_count(), 0);
    assert!(w.live.svc.pool().pages_free() > free0, "the pages are reusable now");
    assert_eq!(w.live.svc.pages_quarantined(), 0);
    // and reuse is real: a new object lands on recycled pages and the (page-exclusive) fake kernel accepts the lease
    let h2 = w.create(inc, 4096);
    let l2 = w.lease(inc, h2, 4096);
    assert_ne!(l2.region_cap, handle, "a lease handle is never reissued");
    assert_eq!(w.leases.live_count(), 1);
    w.raw(inc, Opcode::SessionClose, &[]).unwrap();
    w.wake(Wake::Deadline);
    assert_eq!(w.snapshot(), base, "the session's end took the second lease with it");
}

#[test]
fn pages_whose_lease_the_kernel_will_not_confirm_ended_are_quarantined_never_reused() {
    let mut w = world();
    let (inc, tl, seqno, _h) = w.closed_while_in_flight();
    let free0 = w.live.svc.pool().pages_free();
    w.leases.0.borrow_mut().revoke_fails = true; // an unproven teardown
    w.gpu_complete(tl, seqno);
    w.wake(Wake::Deadline);
    assert_eq!(w.live.svc.pages_quarantined(), 1, "the batch's page");
    assert_eq!(w.live.svc.pool().pages_free(), free0 + 1, "only the unmapped result's page came back: the quarantined page did not");
    assert!(w.live.log.has("QUARANTINED"));
    assert_eq!(w.live.svc.live_kernel_leases(), 0);
    assert_eq!(w.leases.live_count(), 1, "the kernel still has the lease: its pages must never be anyone else's");
    // many new objects mapped: none is handed the quarantined page (the fake kernel refuses a lease over a leased page)
    w.leases.0.borrow_mut().revoke_fails = false;
    for _ in 0..8 {
        let h = w.create(inc, 4096);
        let l = w.lease(inc, h, 4096);
        assert_ne!(l.region_cap, 0);
    }
    assert_eq!(w.leases.live_count(), 9);
}

#[test]
fn a_peer_that_dies_has_its_leases_revoked_at_once_and_its_pages_wait_for_the_gpu() {
    let mut w = world();
    let base = w.snapshot();
    // died with nothing in flight
    let inc = w.open();
    let h = w.create(inc, 4096);
    let _ = w.lease(inc, h, 4096);
    assert_eq!(w.leases.live_count(), 1);
    w.t.kill(w.cid);
    w.wake(Wake::Request(w.cid));
    assert_eq!(w.leases.live_count(), 0, "the dead client's lease is revoked the moment its session is");
    assert_eq!(w.snapshot(), base);
    // died with the GPU still using the mapped object
    w.reconnect();
    let (_inc, tl, seqno) = start_work(&mut w);
    let free0 = w.live.svc.pool().pages_free();
    assert_eq!(w.leases.live_count(), 1);
    w.t.kill(w.cid);
    w.wake(Wake::Request(w.cid));
    assert_eq!(w.leases.live_count(), 0, "revoked at death, though the request is still on the engine");
    assert_eq!(w.live.svc.requests_in_flight(), 1);
    assert_eq!(w.live.svc.pool().pages_free(), free0, "the pages are not reusable until the GPU retires");
    w.gpu_complete(tl, seqno);
    w.wake(Wake::Deadline);
    assert_eq!(w.snapshot(), base);
    assert_eq!(w.live.svc.pages_quarantined(), 0);
}

#[test]
fn a_mapping_the_kernel_will_not_lease_is_refused_by_name_and_leaves_no_gem_mapping_behind() {
    use i915_device::uapi::kernel_lease::{LeaseFail, NoLeaseSys};
    let mut w = world();
    let inc = w.open();
    let h = w.create(inc, 4096);
    let off = w.call(inc, Opcode::GemMmapOffset, &GemMmapOffsetReq { handle: h, flags: 2, ..Default::default() }).body::<GemMmapOffsetRep>();
    let open = GemMmapOpenReq { offset: off.offset, len: 4096 };
    w.leases.0.borrow_mut().create_fails = Some(LeaseFail::NoMemory);
    assert!(w.call(inc, Opcode::GemMmapOpen, &open).is_err(errno::ENOMEM));
    w.leases.0.borrow_mut().create_fails = Some(LeaseFail::Refused { name: "LEASE-PAGE-ALREADY-LEASED", word: 0 });
    assert!(w.call(inc, Opcode::GemMmapOpen, &open).is_err(errno::EBUSY));
    assert!(w.live.log.has("name=LEASE-PAGE-ALREADY-LEASED"));
    w.live.svc.set_lease_sys(Box::new(NoLeaseSys));
    assert!(w.call(inc, Opcode::GemMmapOpen, &open).is_err(errno::ENODEV));
    assert!(w.live.log.has("name=MMAP-LEASE-UNAVAILABLE"));
    assert_eq!((w.live.svc.live_leases(), w.live.svc.live_kernel_leases()), (0, 0));
    // had a GEM mapping been left behind, the object would outlive its last handle
    w.close_handle(inc, h);
    assert_eq!(w.live.svc.gem().object_count(), 0);
}
