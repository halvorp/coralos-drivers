// SPDX-License-Identifier: GPL-2.0-only
//! The production payload-region binding (`SessionRegions` over the kernel's session-scoped REGION_MAP) under the live loop, against a FAKE
//! kernel that enforces what the real one does: a slot is a region delegated over ONE session, a read-only delegation cannot be mapped for
//! write, a session that ended maps nothing, an out-of-region range is refused, and a mapping exists only between map and unmap.
#![cfg(feature = "p3-exec")]
mod exec_common;
use coralos_hal::fake::FakeDma;
use exec_common::*;
use i915_device::live::{ConnId, Live, LiveStats, RegionSys, SessionRegions, Step, SysRefusal, Wake};
use i915_device::uapi::{bring_up_render, Hw, RenderConfig};
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_gt_core::emit::FlushQuirks;
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;
use i915_wire::msg::device::*;
use i915_wire::msg::exec::{self, ExecObject};
use i915_wire::msg::session::*;
use i915_wire::msg::Body;
use i915_wire::region::{RegionRef, MAX_REGION_PAYLOAD};
use i915_wire::{errno, Header, Opcode, Status, FLAG_PAYLOAD_REGION, HEADER_BYTES, MAX_PAYLOAD};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

mod net_common;
use net_common::*;

// ---- the fake kernel ------------------------------------------------------------------------------------------------------------

struct Grant {
    /// The session (its request endpoint) the delegation travelled.
    req_ep: u16,
    bytes: Vec<u8>,
    ro: bool,
}

#[derive(Default)]
struct K {
    grants: BTreeMap<u64, Grant>,
    ended: BTreeSet<u16>,
    live_maps: usize,
    max_live_maps: usize,
    maps: usize,
    reads: Vec<(u64, u64, usize)>,
    writes: Vec<(u64, u64, usize)>,
    /// The client scribbles over its whole region the instant after the device's copy.
    clobber_after_read: Option<u8>,
}

#[derive(Clone, Default)]
struct Kernel(Rc<RefCell<K>>);

impl Kernel {
    fn delegate(&self, slot: u64, req_ep: u16, bytes: Vec<u8>, ro: bool) {
        self.0.borrow_mut().grants.insert(slot, Grant { req_ep, bytes, ro });
    }
    fn bytes(&self, slot: u64) -> Vec<u8> {
        self.0.borrow().grants[&slot].bytes.clone()
    }
    /// The session ends (the client closed it or died): the kernel removes every mapping and refuses new ones.
    fn end_session(&self, req_ep: u16) {
        self.0.borrow_mut().ended.insert(req_ep);
    }
}

impl RegionSys for Kernel {
    fn read(&mut self, cap: u64, offset: u64, out: &mut [u8]) -> Result<(), SysRefusal> {
        let mut k = self.0.borrow_mut();
        let K { grants, ended, .. } = &mut *k;
        let g = grants.get_mut(&cap).ok_or(SysRefusal::NoGrant)?;
        if ended.contains(&g.req_ep) {
            return Err(SysRefusal::Revoked);
        }
        let end = offset.checked_add(out.len() as u64).filter(|e| *e <= g.bytes.len() as u64).ok_or(SysRefusal::OutOfRegion)?;
        out.copy_from_slice(&g.bytes[offset as usize..end as usize]);
        let clobber = k.clobber_after_read;
        if let Some(b) = clobber {
            k.grants.get_mut(&cap).unwrap().bytes.fill(b);
        }
        k.maps += 1;
        k.max_live_maps = k.max_live_maps.max(1); // mapped for the copy, unmapped before the call returns
        k.reads.push((cap, offset, out.len()));
        Ok(())
    }
    fn write(&mut self, cap: u64, offset: u64, data: &[u8]) -> Result<(), SysRefusal> {
        let mut k = self.0.borrow_mut();
        let g = k.grants.get_mut(&cap).ok_or(SysRefusal::NoGrant)?;
        if g.ro {
            return Err(SysRefusal::ReadOnly);
        }
        if k.ended.contains(&k.grants[&cap].req_ep) {
            return Err(SysRefusal::Revoked);
        }
        let g = k.grants.get_mut(&cap).unwrap();
        let end = offset.checked_add(data.len() as u64).filter(|e| *e <= g.bytes.len() as u64).ok_or(SysRefusal::OutOfRegion)?;
        g.bytes[offset as usize..end as usize].copy_from_slice(data);
        k.maps += 1;
        k.max_live_maps = k.max_live_maps.max(1);
        k.writes.push((cap, offset, data.len()));
        Ok(())
    }
}

// ---- the world ------------------------------------------------------------------------------------------------------------------

const GIB: u64 = 1 << 30;
type L = Live<FakeT, SessionRegions<Kernel>, Chip, FakeDma, Mem, Lines>;

struct W {
    live: L,
    t: FakeT,
    k: Kernel,
    cid: ConnId,
    rid: u64,
}

fn world() -> W {
    let (_chip, _mem, mut ex, mut gm) = built();
    let mut log = Lines::default();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    let dev = UapiDevice::new(kbl(), 0x09, sseu, hz, 8 * GIB).unwrap();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    let cfg = RenderConfig { quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, stall_bound_ms: 1000 };
    let svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, cfg).unwrap();
    let t = FakeT::default();
    t.0.borrow_mut().now = 1;
    let k = Kernel::default();
    let mut live = Live::new(t.clone(), SessionRegions::new(k.clone()), svc, ex, gm, log).unwrap();
    let cid = connect(&t, &mut live);
    live.stats = LiveStats::default();
    W { live, t, k, cid, rid: 0 }
}

fn connect(t: &FakeT, live: &mut L) -> ConnId {
    let cid = t.connect();
    t.0.borrow_mut().default = Some(cid);
    t.0.borrow_mut().script.push_back(Ev::Wake(Wake::Accept));
    assert_eq!(live.step(), Step::Woke(Wake::Accept));
    cid
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
    /// The session endpoint the kernel minted for the client of connection `c` (the fake transport's naming).
    fn req_ep(c: ConnId) -> u16 {
        100 + c.0 as u16
    }
    fn deliver_as(&mut self, c: ConnId, f: Vec<u8>) {
        let msgs: Vec<Vec<u8>> = match i915_wire::frag::split(&f, i915_wire::frag::Limits::KERNEL) {
            Ok(m) => m.map(|m| m.to_vec()).collect(),
            Err(_) => vec![f],
        };
        for m in msgs {
            self.t.0.borrow_mut().script.push_back(Ev::To(c, m));
            self.live.step();
        }
    }
    fn deliver(&mut self, f: Vec<u8>) {
        self.deliver_as(self.cid, f)
    }
    fn last_as(&self, c: ConnId) -> (Header, Vec<u8>) {
        let s = self.t.0.borrow();
        let (h, p) = Header::decode(s.clients[&c].sent.last().expect("a reply")).unwrap();
        (h, p.to_vec())
    }
    fn status_errno(&self, c: ConnId) -> (Status, i32) {
        let (_, p) = self.last_as(c);
        (Status::from_u32(u32::from_le_bytes(p[0..4].try_into().unwrap())).unwrap(), i32::from_le_bytes(p[4..8].try_into().unwrap()))
    }
    fn open_as(&mut self, c: ConnId) -> u64 {
        self.rid += 1;
        let mut p = vec![0u8; 64];
        let n = SessionOpenReq::default().to_payload(&mut p).unwrap();
        let mut f = vec![0u8; HEADER_BYTES + n];
        f[HEADER_BYTES..].copy_from_slice(&p[..n]);
        Header::request(Opcode::SessionOpen, self.rid, 0, n as u32).encode(&mut f).unwrap();
        self.deliver_as(c, f);
        self.last_as(c).0.session_incarnation
    }
    /// What GETPARAM answers when it travels inline: the reference for what the same call answers through a region.
    fn inline_get_param(&mut self, c: ConnId, inc: u64) -> (Status, i32) {
        self.rid += 1;
        self.deliver_as(c, frame(Opcode::GetParam, self.rid, inc, &get_param_payload()));
        self.status_errno(c)
    }
    fn open(&mut self) -> u64 {
        self.open_as(self.cid)
    }
}

fn get_param_payload() -> Vec<u8> {
    let mut p = vec![0u8; 64];
    let n = GetParamReq { param: 1, ..Default::default() }.to_payload(&mut p).unwrap();
    p.truncate(n);
    p
}

const PINNED: u64 = 1 << 4;
const SUPPORTS_48B: u64 = 1 << 3;

/// The largest EXECBUFFER2 the wire allows: 1024 objects, 1024 relocations and 256 fences.
fn max_exec_payload() -> Vec<u8> {
    let mut objs: Vec<ExecObject> = (0..1024).map(|i| ExecObject { handle: 7000 + i, offset: 0x4000_0000 + i as u64 * 0x1000, flags: PINNED | SUPPORTS_48B, ..Default::default() }).collect();
    objs[0].relocation_count = 1024;
    let relocs: Vec<exec::Reloc> = (0..1024).map(|_| exec::Reloc::default()).collect();
    let fences: Vec<exec::Fence> = (0..256).map(|i| exec::Fence { handle: 1 + i, flags: 1 }).collect();
    let hdr = exec::ExecHeader { batch_len: 24, flags: exec::flags::NO_RELOC | exec::flags::FENCE_ARRAY, ..Default::default() };
    let mut out = vec![0u8; 200_000];
    let n = exec::encode_request(&mut out, hdr, &objs, &relocs, &fences, &[]).unwrap();
    out.truncate(n);
    out
}

// ---- tests ----------------------------------------------------------------------------------------------------------------------

#[test]
fn an_84_kib_execbuffer2_is_read_from_a_delegated_region_in_one_snapshot_and_the_mapping_is_gone_after() {
    let mut w = world();
    let inc = w.open();
    let big = max_exec_payload();
    assert!(big.len() >= 84_000 && big.len() > MAX_PAYLOAD as usize && big.len() as u32 <= MAX_REGION_PAYLOAD, "the largest call the wire allows: {}", big.len());
    let ep = W::req_ep(w.cid);
    // request storage delegated READ-ONLY (the payload sits at 4096), reply storage READ-WRITE: two slots of the device's table
    w.k.delegate(3, ep, [vec![0xEE; 4096], big.clone()].concat(), true);
    w.k.delegate(4, ep, vec![0; 8192], false);
    let rr = RegionRef { region: 3, offset: 4096, len: big.len() as u32, reply_cap: 8192, reply_offset: 0, reply_region: 4 };
    w.rid += 1;
    let id = w.rid;
    w.deliver(region_frame(Opcode::Execbuffer2, id, inc, &rr));
    assert_eq!(w.k.0.borrow().reads, vec![(3, 4096, big.len())], "the device copied exactly the named bytes, once");
    let (h, _) = w.last_as(w.cid);
    assert_eq!(h.request_id, id);
    let (status, err) = w.status_errno(w.cid);
    assert_eq!(status, Status::Ok, "decoded in full; log {:?}", w.live.log.0.iter().rev().take(3).collect::<Vec<_>>());
    assert_ne!(err, 0, "and refused the bogus object handles");
    assert!(w.live.log.has("op=EXECBUFFER2 refused"));
    assert_eq!((w.live.stats.region_calls, w.live.stats.refused), (1, 0));
    // one byte short is a different, malformed call: the length that was read is the length that was declared
    w.rid += 1;
    w.deliver(region_frame(Opcode::Execbuffer2, w.rid, inc, &RegionRef { len: big.len() as u32 - 10, ..rr }));
    assert_eq!(w.status_errno(w.cid).0, Status::Malformed);
    let k = w.k.0.borrow();
    assert_eq!((k.live_maps, k.max_live_maps), (0, 1), "a mapping exists only for the copy: none is kept");
    assert!(k.writes.is_empty(), "nothing was written into a read-only request region");
}

#[test]
fn a_request_the_client_changes_after_delegation_cannot_change_what_the_device_validated() {
    let mut w = world();
    let inc = w.open();
    let ep = W::req_ep(w.cid);
    let req = get_param_payload();
    let want = w.inline_get_param(w.cid, inc);
    assert_eq!(want.0, Status::Ok);
    w.k.delegate(3, ep, [req.clone(), vec![0; 64]].concat(), true);
    w.k.delegate(4, ep, vec![0; 512], false);
    // the client scribbles over its region the instant after the device's copy
    w.k.0.borrow_mut().clobber_after_read = Some(0xFF);
    let rr = RegionRef { region: 3, offset: 0, len: req.len() as u32, reply_cap: 512, reply_offset: 0, reply_region: 4 };
    w.rid += 1;
    w.deliver(region_frame(Opcode::GetParam, w.rid, inc, &rr));
    assert_eq!(w.status_errno(w.cid), want, "the device answered the bytes it copied, not the bytes the client left");
    assert_eq!(w.k.0.borrow().reads.len(), 1, "one read: there is no second fetch to race");
    assert!(w.k.bytes(3).iter().all(|b| *b == 0xFF), "the client did change its memory");
    // what the device would have seen had it read the live region: the same call over the scribbled bytes is not the call that ran
    w.rid += 1;
    w.deliver(region_frame(Opcode::GetParam, w.rid, inc, &rr));
    assert_ne!(w.status_errno(w.cid), want);
}

#[test]
fn a_reply_too_large_for_a_frame_is_written_into_the_clients_separate_read_write_region() {
    let mut w = world();
    let inc = w.open();
    let ep = W::req_ep(w.cid);
    w.live.inline_reply_max = 8; // an ordinary reply "does not fit": the path a long QUERY takes
    let req = get_param_payload();
    w.k.delegate(3, ep, req.clone(), true);
    w.k.delegate(4, ep, vec![0xAB; 512], false);
    let rr = RegionRef { region: 3, offset: 0, len: req.len() as u32, reply_cap: 512, reply_offset: 16, reply_region: 4 };
    w.rid += 1;
    w.deliver(region_frame(Opcode::GetParam, w.rid, inc, &rr));
    let (h, p) = w.last_as(w.cid);
    assert!(h.is_region() && h.is_reply());
    let placed = RegionRef::decode(&p).unwrap();
    assert_eq!((placed.region, placed.offset), (4, 16), "the reply says WHICH region holds it");
    let out = w.k.bytes(4);
    assert_eq!(&out[..16], &[0xAB; 16][..], "bytes before the reply area are untouched");
    assert_eq!(i915_wire::codec::Reader::new(&out[16..]).u32("status").unwrap(), Status::Ok as u32);
    assert_eq!(w.k.bytes(3), req, "the request region was only read");
    assert_eq!(w.k.0.borrow().writes, vec![(4, 16, placed.len as usize)]);
    // a reply region delegated READ-ONLY is refused by name, and nothing is written anywhere
    w.k.delegate(5, ep, vec![0xCD; 512], true);
    w.rid += 1;
    w.deliver(region_frame(Opcode::GetParam, w.rid, inc, &RegionRef { reply_region: 5, ..rr }));
    assert_eq!(w.status_errno(w.cid), (Status::Ok, -errno::EACCES));
    assert!(w.live.log.has("name=PAYLOAD-REGION-READ-ONLY"));
    assert_eq!(w.k.bytes(5), vec![0xCD; 512]);
}

#[test]
fn a_connection_reaches_only_the_slots_it_uses_and_a_dead_session_maps_nothing() {
    let mut w = world();
    let a = w.cid;
    let ia = w.open_as(a);
    let b = connect(&w.t, &mut w.live);
    let ib = w.open_as(b);
    let req = get_param_payload();
    w.k.delegate(3, W::req_ep(a), req.clone(), true);
    w.k.delegate(4, W::req_ep(a), vec![0; 512], false);
    let rr = RegionRef { region: 3, offset: 0, len: req.len() as u32, reply_cap: 512, reply_offset: 0, reply_region: 4 };
    let want = w.inline_get_param(a, ia);
    w.rid += 1;
    w.deliver_as(a, region_frame(Opcode::GetParam, w.rid, ia, &rr));
    assert_eq!(w.status_errno(a), want, "the same answer as the inline call");
    assert_eq!(w.live.regions.claimed(), 1, "the slot A used is A's");
    // B names A's slots: refused by name, and nothing of A's is read or written
    let reads = w.k.0.borrow().reads.len();
    w.live.inline_reply_max = 8;
    w.rid += 1;
    w.deliver_as(b, region_frame(Opcode::GetParam, w.rid, ib, &rr));
    assert_eq!(w.status_errno(b), (Status::Ok, -errno::EACCES));
    assert!(w.live.log.has("name=PAYLOAD-REGION-NOT-YOURS"));
    assert_eq!(w.k.0.borrow().reads.len(), reads);
    assert!(w.k.0.borrow().writes.is_empty());
    // A dies: the kernel ends its session (every mapping goes) and the loop reaps the connection
    w.k.end_session(W::req_ep(a));
    w.t.kill(a);
    w.t.0.borrow_mut().script.push_back(Ev::Wake(Wake::Request(a)));
    w.live.step();
    assert_eq!(w.live.stats.conns_reaped, 1);
    assert_eq!(w.live.regions.claimed(), 0, "A's bookkeeping went with it");
    assert_eq!(w.live.svc.live_sessions(), 1, "only B's session is left");
    // B now names the dead session's slot: the KERNEL refuses it (session ended) - a named refusal, not a stale pointer
    w.rid += 1;
    w.deliver_as(b, region_frame(Opcode::GetParam, w.rid, ib, &rr));
    assert_eq!(w.status_errno(b), (Status::Ok, -errno::ENODEV));
    assert!(w.live.log.has("name=PAYLOAD-REGION-REVOKED"));
    assert_eq!(w.live.regions.claimed(), 0, "a refused access claims nothing");
    assert_eq!(w.k.0.borrow().live_maps, 0);
}

#[test]
fn the_kernels_refusals_are_told_apart_by_name() {
    use i915_device::live::{PayloadRegions, RegionError};
    let mut r = SessionRegions::new(Kernel::default());
    let k = r.sys.clone();
    k.delegate(1, 100, vec![7; 64], true);
    let c = ConnId(1);
    let mut buf = [0u8; 16];
    assert_eq!(r.read(c, 1, 0, &mut buf), Ok(()));
    assert_eq!(buf, [7; 16]);
    assert_eq!(r.read(c, 1, 60, &mut buf), Err(RegionError::OutOfRange { region: 1, offset: 60, len: 16, size: 0 }));
    assert_eq!(r.read(c, 9, 0, &mut buf), Err(RegionError::Unknown { region: 9 }));
    assert_eq!(r.write(c, 1, 0, &[1]), Err(RegionError::ReadOnly { region: 1 }));
    k.end_session(100);
    assert_eq!(r.read(c, 1, 0, &mut buf), Err(RegionError::Revoked { region: 1 }));
    // a slot another connection holds
    k.delegate(2, 101, vec![0; 64], false);
    assert_eq!(r.write(ConnId(2), 2, 0, &[1]), Ok(()));
    assert_eq!(r.write(c, 2, 0, &[1]), Err(RegionError::NotYours { region: 2 }));
    r.conn_ended(ConnId(2));
    assert_eq!(r.write(c, 2, 0, &[1]), Ok(()), "released with its connection");
    let names = [
        RegionError::Revoked { region: 0 }.name(),
        RegionError::NotYours { region: 0 }.name(),
        RegionError::ReadOnly { region: 0 }.name(),
        RegionError::NoMemory { what: "x" }.name(),
        RegionError::Refused { name: "REGION-BAD-RIGHTS", word: 1 }.name(),
    ];
    assert_eq!(names, ["PAYLOAD-REGION-REVOKED", "PAYLOAD-REGION-NOT-YOURS", "PAYLOAD-REGION-READ-ONLY", "PAYLOAD-REGION-NOMEM", "REGION-BAD-RIGHTS"]);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn every_status_word_of_the_kernel_has_a_device_meaning() {
    use coralos_rt::region::{LeaseRefusal, LeaseStatus, RegionRefusal, RegionStatus};
    use i915_device::live::rt_regions::classify;
    use i915_device::uapi::kernel_lease::LeaseFail;
    use i915_device::uapi::rt_lease::classify as classify_lease;
    for s in RegionStatus::ALL {
        let want = match s {
            RegionStatus::SessionEnded | RegionStatus::RevokedDuringMap | RegionStatus::RegionGone => SysRefusal::Revoked,
            RegionStatus::OutOfRegion => SysRefusal::OutOfRegion,
            RegionStatus::NoGrant => SysRefusal::NoGrant,
            RegionStatus::RightsExceedGrant => SysRefusal::ReadOnly,
            RegionStatus::NoMemory | RegionStatus::NoVirtualSpace => SysRefusal::NoMemory,
            RegionStatus::BadRights | RegionStatus::NotMapped => SysRefusal::Other { name: s.name(), word: s.code() },
        };
        assert_eq!(classify(RegionRefusal::Named(s)), want, "{s:?}");
    }
    assert_eq!(classify(RegionRefusal::Unknown(5)), SysRefusal::Other { name: "REGION-UNKNOWN-WORD", word: 5 });
    for s in LeaseStatus::ALL {
        let want = if s == LeaseStatus::NoMemory { LeaseFail::NoMemory } else { LeaseFail::Refused { name: s.name(), word: s.code() } };
        assert_eq!(classify_lease(LeaseRefusal::Named(s)), want, "{s:?}");
    }
    assert_eq!(classify_lease(LeaseRefusal::Unknown(9)), LeaseFail::Refused { name: "LEASE-UNKNOWN-WORD", word: 9 });
}
