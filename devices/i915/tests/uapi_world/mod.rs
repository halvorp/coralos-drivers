// SPDX-License-Identifier: GPL-2.0-only
//! The simulated device the uAPI suites drive through wire frames (moved out of uapi_service.rs so a second suite can share it): the world,
//! the call helpers, and the test playing the GPU (`gpu_complete`).
#![allow(dead_code, unused_imports)]
use crate::exec_common::*;
use coralos_hal::fake::FakeDma;
use i915_device::exec::dma::ObjKind;
use i915_device::uapi::pool::HostFree;
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


pub const BATCH_VA: u64 = 0x1000_0000_0000;
pub const RESULT_VA: u64 = 0x2000_0000;
pub const VALUE: u32 = 0xc0ff_ee42;
pub const MI_STORE_DWORD_IMM_GEN4: u32 = (0x20 << 23) | 2; // intel_gpu_commands.h:136
pub const MI_BATCH_BUFFER_END: u32 = 0x0a << 23; // intel_gpu_commands.h:62
pub const PINNED: u64 = 1 << 4;
pub const SUPPORTS_48B: u64 = 1 << 3;
pub const WRITE: u64 = 1 << 2;
pub const FENCE_SIGNAL: u32 = 2;
pub const FENCE_WAIT: u32 = 1;
pub const GIB: u64 = 1 << 30;

pub struct W {
    pub chip: Chip,
    pub mem: Mem,
    pub ex: i915_device::exec::Executor<Chip>,
    pub gm: Gm,
    pub log: Lines,
    pub svc: UapiService,
    pub leases: FakeLeases,
    pub now: u64,
    pub csb: u32,
    pub rid: u64,
}

#[macro_export]
macro_rules! hw {
    ($w:ident) => {
        Hw { ex: &mut $w.ex, gm: &mut $w.gm, log: &mut $w.log, now_ms: $w.now }
    };
}

pub fn world() -> W {
    world_with(ENDOWMENT)
}

/// The simulated device with `endowment` bytes of DMA grant.
pub fn world_with(endowment: usize) -> W {
    world_on(FakeDma::new(PHYS_BASE), endowment)
}

/// The simulated device on a given fake host (`can_free = false` poses as a host with no free call).
pub fn world_on(dma: FakeDma, endowment: usize) -> W {
    let (chip, mem, mut ex, mut gm) = built();
    gm.alloc = i915_device::exec::dma::GpuAlloc::new(dma, &kbl(), endowment);
    let mut log = Lines::default();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    let dev = UapiDevice::new(kbl(), 0x09, sseu, hz, 8 * GIB).unwrap();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    let cfg = RenderConfig { quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, stall_bound_ms: 1000, keep: None };
    let mut svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, cfg).unwrap();
    let leases = FakeLeases::default();
    svc.set_lease_sys(Box::new(leases.clone()));
    W { chip, mem, ex, gm, log, svc, leases, now: 1, csb: 0, rid: 0 }
}

impl W {
    /// Pages the fake host counts against the device now.
    pub fn dma_held(&self) -> u64 {
        self.gm.alloc.dma().pages_held()
    }
    pub fn dma_freed(&self) -> &[coralos_hal::DmaGrant] {
        &self.gm.alloc.dma().freed
    }
}

/// What a call answered.
pub struct Resp {
    pub hdr: Header,
    pub status: Status,
    pub errno: i32,
    pub body: Vec<u8>,
    /// status + errno + body, as the reply payload
    pub payload: Vec<u8>,
}

impl Resp {
    pub fn body<B: Body>(&self) -> B {
        assert_eq!((self.status, self.errno), (Status::Ok, 0), "the call failed: {:?} errno {}", self.status, self.errno);
        B::from_payload(&self.body).unwrap()
    }
    pub fn is_err(&self, e: i32) -> bool {
        self.status == Status::Ok && self.errno == -e
    }
}

pub fn parse(frame: &[u8]) -> Resp {
    let (hdr, p) = Header::decode(frame).unwrap();
    assert!(hdr.is_reply());
    let (status, errno) = (Status::from_u32(u32::from_le_bytes(p[0..4].try_into().unwrap())).unwrap(), i32::from_le_bytes(p[4..8].try_into().unwrap()));
    Resp { hdr, status, errno, body: p[8..].to_vec(), payload: p.to_vec() }
}

/// Send a raw payload. `None` = the call parked.
pub fn send_raw(w: &mut W, inc: u64, op: Opcode, payload: &[u8]) -> Option<Resp> {
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

pub fn call<B: Body>(w: &mut W, inc: u64, op: Opcode, req: &B) -> Resp {
    let mut p = vec![0u8; 4096];
    let n = req.to_payload(&mut p).unwrap();
    send_raw(w, inc, op, &p[..n]).expect("the call parked")
}

pub fn open(w: &mut W) -> u64 {
    let r = call(w, 0, Opcode::SessionOpen, &SessionOpenReq::default());
    assert_eq!(r.hdr.session_incarnation, r.body::<SessionOpenRep>().incarnation, "the reply header carries the new incarnation");
    let inc = r.body::<SessionOpenRep>().incarnation;
    w.svc.set_session_peer(inc, peer_for(inc)); // what the live loop does from the connect announce
    inc
}

pub fn create(w: &mut W, inc: u64, size: u64) -> u32 {
    call(w, inc, Opcode::GemCreate, &GemCreateReq { size }).body::<GemCreateRep>().handle
}

/// A CPU lease on the whole object.
pub fn lease(w: &mut W, inc: u64, handle: u32, len: u64) -> GemMmapRep {
    let off = call(w, inc, Opcode::GemMmapOffset, &GemMmapOffsetReq { handle, flags: 2, ..Default::default() }).body::<GemMmapOffsetRep>();
    call(w, inc, Opcode::GemMmapOpen, &GemMmapOpenReq { offset: off.offset, len }).body::<GemMmapRep>()
}

pub fn syncobj(w: &mut W, inc: u64, flags: u32) -> u32 {
    call(w, inc, Opcode::SyncobjCreate, &SyncobjCreateReq { flags, ..Default::default() }).body::<SyncobjHandleRep>().handle
}

pub fn write_batch(w: &mut W, batch_lease: &GemMmapRep) {
    let va = RESULT_VA;
    for (i, d) in [MI_STORE_DWORD_IMM_GEN4, va as u32, (va >> 32) as u32, VALUE, MI_BATCH_BUFFER_END].iter().enumerate() {
        w.gm.store32(batch_lease.grant_addr + 4 * i as u64, *d).unwrap();
    }
}

/// An EXECBUFFER2 payload: `[result, batch]`, both softpinned, the batch last; `fences` are `(syncobj, flags)`.
pub fn exec_payload(result: u32, batch: u32, fences: &[(u32, u32)], timeline: Option<(u32, u64)>, ctx: u32) -> Vec<u8> {
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

pub fn submit(w: &mut W, inc: u64, payload: &[u8]) -> (Resp, Option<exec::ExecRepHead>) {
    let r = send_raw(w, inc, Opcode::Execbuffer2, payload).unwrap();
    let head = (r.errno == 0 && r.status == Status::Ok).then(|| ExecReply::decode(&r.payload).unwrap().head);
    (r, head)
}

/// The hardware picks up the oldest submission and completes it: the CSB promotes then retires, the context's timeline reaches `seqno`.
pub fn gpu_complete(w: &mut W, timeline_dev: u64, seqno: u32) {
    let hwsp = *w.gm.alloc.grants().find(|g| g.kind == ObjKind::Hwsp).unwrap();
    let first = w.csb;
    let entry = |i: u32| hwsp.cpu_mapping + (0x10 + 2 * (i % 6)) as u64 * 4;
    w.mem.put(entry(first), 1);
    w.mem.put(entry(first) + 4, 0);
    w.mem.put(entry(first + 1), 1 << 3);
    w.mem.put(entry(first + 1) + 4, 0);
    let tl_cpu = w.gm.alloc.cpu_of(timeline_dev, 4).expect("the timeline page is a granted page");
    w.mem.put(tl_cpu, seqno); // the GPU writes the seqno through the context's OWN timeline page
    w.mem.put(hwsp.cpu_mapping + 0x1f * 4, (first + 1) % 6);
    w.csb += 2;
}

pub fn poll(w: &mut W) {
    w.now += 1;
    w.svc.poll(&mut hw!(w));
}

pub fn refused(w: &W, name: &str) -> bool {
    w.log.0.iter().any(|l| l.contains("refused") && l.contains(name))
}

// ---- the end-to-end path ----------------------------------------------------------------------------------------------------

