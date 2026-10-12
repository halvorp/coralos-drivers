// SPDX-License-Identifier: MIT OR Apache-2.0
//! The uAPI messages: pinned opcode numbers, hand-written bytes, and the properties every message must have (transactional decode,
//! no silently ignored byte, explicit zero padding, named refusals with the Linux errno).
use i915_wire::codec::{Reader, Writer};
use i915_wire::msg::ctx::*;
use i915_wire::msg::device::*;
use i915_wire::msg::exec::{self, *};
use i915_wire::msg::gem::*;
use i915_wire::msg::prime::*;
use i915_wire::msg::session::*;
use i915_wire::msg::sync::{self, *};
use i915_wire::msg::{self, *};
use i915_wire::*;

#[test]
fn every_opcode_number_is_pinned_and_introduced_by_the_right_minor() {
    // written out by hand (PORT_CONVENTIONS section 3): the numbers are the ABI
    let table: &[(Opcode, u32, u16, &str)] = &[
        (Opcode::Hello, 0, 0, "HELLO"),
        (Opcode::SubmitBatch, 5, 0, "SUBMIT-BATCH"),
        (Opcode::SessionOpen, 0x100, 1, "SESSION-OPEN"),
        (Opcode::SessionClose, 0x101, 1, "SESSION-CLOSE"),
        (Opcode::GetParam, 0x110, 1, "GETPARAM"),
        (Opcode::Query, 0x111, 1, "QUERY"),
        (Opcode::RegRead, 0x112, 1, "REG-READ"),
        (Opcode::GemCreate, 0x120, 1, "GEM-CREATE"),
        (Opcode::GemCreateExt, 0x121, 1, "GEM-CREATE-EXT"),
        (Opcode::GemClose, 0x122, 1, "GEM-CLOSE"),
        (Opcode::GemMmapOffset, 0x123, 1, "GEM-MMAP-OFFSET"),
        (Opcode::GemMmapOpen, 0x124, 1, "GEM-MMAP-OPEN"),
        (Opcode::GemMmapLegacy, 0x125, 1, "GEM-MMAP-LEGACY"),
        (Opcode::GemMmapClose, 0x126, 1, "GEM-MMAP-CLOSE"),
        (Opcode::GemSetDomain, 0x127, 1, "GEM-SET-DOMAIN"),
        (Opcode::GemSetTiling, 0x128, 1, "GEM-SET-TILING"),
        (Opcode::GemGetTiling, 0x129, 1, "GEM-GET-TILING"),
        (Opcode::GemBusy, 0x12a, 1, "GEM-BUSY"),
        (Opcode::GemWait, 0x12b, 1, "GEM-WAIT"),
        (Opcode::GemMadvise, 0x12c, 1, "GEM-MADVISE"),
        (Opcode::GemSetCaching, 0x12d, 1, "GEM-SET-CACHING"),
        (Opcode::GemGetCaching, 0x12e, 1, "GEM-GET-CACHING"),
        (Opcode::ContextCreate, 0x130, 1, "CONTEXT-CREATE"),
        (Opcode::ContextCreateExt, 0x131, 1, "CONTEXT-CREATE-EXT"),
        (Opcode::ContextDestroy, 0x132, 1, "CONTEXT-DESTROY"),
        (Opcode::ContextGetParam, 0x133, 1, "CONTEXT-GETPARAM"),
        (Opcode::ContextSetParam, 0x134, 1, "CONTEXT-SETPARAM"),
        (Opcode::VmCreate, 0x135, 1, "VM-CREATE"),
        (Opcode::VmDestroy, 0x136, 1, "VM-DESTROY"),
        (Opcode::GetResetStats, 0x137, 1, "GET-RESET-STATS"),
        (Opcode::Execbuffer2, 0x140, 1, "EXECBUFFER2"),
        (Opcode::SyncobjCreate, 0x150, 1, "SYNCOBJ-CREATE"),
        (Opcode::SyncobjDestroy, 0x151, 1, "SYNCOBJ-DESTROY"),
        (Opcode::SyncobjWait, 0x152, 1, "SYNCOBJ-WAIT"),
        (Opcode::SyncobjTimelineWait, 0x153, 1, "SYNCOBJ-TIMELINE-WAIT"),
        (Opcode::SyncobjSignal, 0x154, 1, "SYNCOBJ-SIGNAL"),
        (Opcode::SyncobjReset, 0x155, 1, "SYNCOBJ-RESET"),
        (Opcode::SyncobjTimelineSignal, 0x156, 1, "SYNCOBJ-TIMELINE-SIGNAL"),
        (Opcode::SyncobjQuery, 0x157, 1, "SYNCOBJ-QUERY"),
        (Opcode::SyncobjTransfer, 0x158, 1, "SYNCOBJ-TRANSFER"),
        (Opcode::SyncobjHandleToFd, 0x159, 1, "SYNCOBJ-HANDLE-TO-FD"),
        (Opcode::SyncobjFdToHandle, 0x15a, 1, "SYNCOBJ-FD-TO-HANDLE"),
        (Opcode::FdClose, 0x15b, 1, "FD-CLOSE"),
        (Opcode::SyncFileMerge, 0x15c, 1, "SYNC-FILE-MERGE"),
        (Opcode::SyncFileWait, 0x15d, 1, "SYNC-FILE-WAIT"),
        (Opcode::PrimeHandleToFd, 0x160, 1, "PRIME-HANDLE-TO-FD"),
        (Opcode::PrimeFdToHandle, 0x161, 1, "PRIME-FD-TO-HANDLE"),
        (Opcode::DmabufExportSyncFile, 0x162, 1, "DMABUF-EXPORT-SYNC-FILE"),
        (Opcode::DmabufImportSyncFile, 0x163, 1, "DMABUF-IMPORT-SYNC-FILE"),
    ];
    for (op, n, since, name) in table {
        assert_eq!(*op as u32, *n);
        assert_eq!(Opcode::from_u32(*n), Ok(*op));
        assert_eq!(op.since_minor(), *since);
        assert_eq!(op.name(), *name);
        assert!(Opcode::ALL.contains(op));
    }
    // the table is the whole family except the four skeleton opcodes it skips (GetReadout..AllocScanout are pinned in wire.rs)
    assert_eq!(Opcode::ALL.len(), table.len() + 4);
    let mut seen: Vec<u32> = Opcode::ALL.iter().map(|o| *o as u32).collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), Opcode::ALL.len(), "no two opcodes share a number");
    assert_eq!(Opcode::from_u32(0x1ff), Err(WireError::UnknownOpcode { got: 0x1ff }));
}

#[test]
fn a_frame_may_not_use_an_opcode_newer_than_its_minor() {
    let mut buf = [0u8; HEADER_BYTES];
    let mut h = Header::request(Opcode::GemCreate, 1, 1, 0);
    h.minor = 0;
    h.encode(&mut buf).unwrap();
    let e = Header::decode_header(&buf).unwrap_err();
    assert_eq!(e, WireError::OpcodeNewerThanMinor { opcode: 0x120, since: 1, got: 0 });
    assert_eq!((e.name(), e.status(), e.errno()), ("WIRE-OPCODE-NEWER-THAN-MINOR", Status::Malformed, errno::EINVAL));
    h.minor = 1;
    h.encode(&mut buf).unwrap();
    assert!(Header::decode_header(&buf).is_ok());
    // a skeleton opcode still decodes under minor 0
    let mut old = Header::request(Opcode::SubmitBatch, 1, 1, 0);
    old.minor = 0;
    old.encode(&mut buf).unwrap();
    assert!(Header::decode_header(&buf).is_ok());
}

#[test]
fn hand_written_bytes_decode_field_by_field() {
    // GEM-CREATE reply: handle 0x00100003, size 0x2000 (after the 8-byte reply head: Ok, errno 0)
    let mut b = vec![];
    b.extend_from_slice(&0u32.to_le_bytes()); // status Ok
    b.extend_from_slice(&0i32.to_le_bytes()); // errno 0
    b.extend_from_slice(&[0x03, 0x00, 0x10, 0x00]); // handle
    b.extend_from_slice(&[0, 0, 0, 0]); // pad
    b.extend_from_slice(&[0x00, 0x20, 0, 0, 0, 0, 0, 0]); // size
    let r = decode_reply::<GemCreateRep>(&b).unwrap();
    assert_eq!((r.status, r.errno, r.body.handle, r.body.size), (Status::Ok, 0, 0x0010_0003, 0x2000));
    // a refused call: errno -ENOENT, zero body
    let mut out = [0xffu8; 64];
    let n = encode_reply::<GemCreateRep>(&mut out, Status::Ok, -errno::ENOENT, None).unwrap();
    assert_eq!(n, 8 + 16);
    assert_eq!(&out[..8], &[0, 0, 0, 0, 0xfe, 0xff, 0xff, 0xff]);
    assert!(out[8..n].iter().all(|x| *x == 0));
    // GEM-WAIT request: handle 7, flags 0, timeout -1 (forever)
    let rq = GemWaitReq { handle: 7, flags: 0, timeout_ns: -1 };
    let mut o = [0u8; 16];
    assert_eq!(rq.to_payload(&mut o).unwrap(), 16);
    assert_eq!(o, [7, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    assert_eq!(GemWaitReq::from_payload(&o), Ok(rq));
}

macro_rules! every_body {
    ($($t:ty),* $(,)?) => {{
        let mut n = 0;
        $( check_body::<$t>(stringify!($t)); n += 1; )*
        n
    }};
}

/// The properties every fixed message has.
fn check_body<B: Body + Default + PartialEq + core::fmt::Debug>(name: &str) {
    let mut out = vec![0xaau8; B::SIZE + 8];
    let n = B::default().to_payload(&mut out).unwrap();
    assert_eq!(n, B::SIZE, "{name}: SIZE is the encoded length");
    assert!(out[..n].iter().all(|x| *x == 0), "{name}: the default encodes to zeroes");
    assert_eq!(B::from_payload(&out[..n]), Ok(B::default()), "{name}");
    if B::SIZE > 0 {
        assert!(matches!(B::from_payload(&out[..n - 1]), Err(WireError::Truncated { .. })), "{name}: one byte short is Truncated");
        let e = B::default().to_payload(&mut out[..n - 1]).unwrap_err();
        assert!(matches!(e, WireError::OutputTooSmall { .. }), "{name}: encode into a short buffer is refused");
    }
    assert!(matches!(B::from_payload(&out[..n + 1]), Err(WireError::TrailingBytes { extra: 1 })), "{name}: one byte long is TrailingBytes");
    // NO BYTE IS IGNORED: set every aligned word to all-ones; the decode either refuses (a pad) or the value re-encodes to the same bytes
    for w in 0..B::SIZE / 4 {
        let mut p = vec![0u8; B::SIZE];
        p[w * 4..w * 4 + 4].copy_from_slice(&[0xff; 4]);
        match B::from_payload(&p) {
            Err(WireError::FieldReservedNonZero { .. }) => {}
            Ok(v) => {
                let mut q = vec![0u8; B::SIZE];
                v.to_payload(&mut q).unwrap();
                assert_eq!(q, p, "{name}: word {w} was silently dropped by decode");
            }
            Err(e) => panic!("{name}: word {w}: unexpected {e:?}"),
        }
    }
}

#[test]
fn every_fixed_message_is_transactional_and_ignores_no_byte() {
    let n = every_body!(
        Empty, UnsupportedBody, SessionOpenReq, SessionOpenRep, GetParamReq, GetParamRep, RegReadReq, RegReadRep, GemCreateReq, GemCreateRep, GemCreateExtHead, GemCloseReq,
        GemMmapOffsetReq, GemMmapOffsetRep, GemMmapOpenReq, GemMmapLegacyReq, GemMmapRep, GemMmapCloseReq, GemSetDomainReq, GemSetTilingReq, GemGetTilingReq, TilingRep,
        GemBusyReq, GemBusyRep, GemWaitReq, GemWaitRep, GemMadviseReq, GemMadviseRep, GemSetCachingReq, GemGetCachingReq, GemCachingRep, ContextCreateReq, ContextRep,
        ContextCreateExtHead, ContextDestroyReq, ContextGetParamReq, ContextGetParamRep, VmCreateReq, VmCreateRep, VmDestroyReq, GetResetStatsReq, GetResetStatsRep,
        exec::ExecHeader, ExecObject, Reloc, exec::Fence, ExtHeader, ExecRepHead, RelocUpdate, SyncobjCreateReq, SyncobjHandleRep, SyncobjDestroyReq, SyncobjWaitHead,
        SyncobjWaitRep, SyncobjArrayHead, SyncobjQueryRepHead, SyncobjTransferReq, SyncobjHandleToFdReq, SyncobjTokenRep, SyncobjFdToHandleReq, FdCloseReq, SyncFileMergeReq,
        SyncFileTokenRep, SyncFileWaitReq, SyncFileWaitRep, PrimeHandleToFdReq, PrimeTokenRep, PrimeFdToHandleReq, PrimeHandleRep, DmabufExportSyncFileReq, DmabufSyncFileRep,
        DmabufImportSyncFileReq,
    );
    assert_eq!(n, 69, "the message count is pinned: a message added or dropped changes this");
}

#[test]
fn the_documented_sizes_are_the_wire_sizes() {
    assert_eq!(<exec::ExecHeader as Body>::SIZE, exec::HEADER_BYTES);
    assert_eq!(<ExecObject as Body>::SIZE, OBJECT_BYTES);
    assert_eq!(<Reloc as Body>::SIZE, RELOC_BYTES);
    assert_eq!(<exec::Fence as Body>::SIZE, FENCE_BYTES);
    assert_eq!(<ExtHeader as Body>::SIZE, EXT_HEADER_BYTES);
    assert_eq!((exec::HEADER_BYTES, OBJECT_BYTES, RELOC_BYTES, FENCE_BYTES, EXT_HEADER_BYTES), (64, 48, 32, 8, 24));
    assert_eq!(<GemCreateExtHead as Body>::SIZE, 32);
    assert_eq!(QUERY_ITEM_BYTES, 32);
    assert_eq!(REPLY_HEAD_BYTES, 8);
}

#[test]
fn a_nonzero_pad_is_refused_by_name() {
    let mut b = [0u8; 8];
    b[4] = 1; // GetParamReq { param, _rsvd }: the reserved word
    let e = GetParamReq::from_payload(&b).unwrap_err();
    assert_eq!(e, WireError::FieldReservedNonZero { what: "GetParamReq._rsvd", got: 1 });
    assert_eq!((e.name(), e.errno()), ("WIRE-FIELD-RESERVED-NONZERO", errno::EINVAL));
    assert_eq!(WireError::Truncated { what: "x", need: 2, have: 1 }.errno(), errno::EFAULT);
}

#[test]
fn gem_create_ext_carries_a_bounded_region_list() {
    let mut out = [0u8; 256];
    let head = GemCreateExtHead { size: 4096, flags: 1, protected: 0, set_pat_present: 1, set_pat: 5, ..Default::default() };
    let n = GemCreateExtReq::encode(&mut out, head, &[(0, 0), (1, 2)]).unwrap();
    assert_eq!(n, 32 + 8);
    let r = GemCreateExtReq::decode(&out[..n]).unwrap();
    assert_eq!((r.head.size, r.head.region_count, r.head.set_pat), (4096, 2, 5));
    assert_eq!((r.region(0), r.region(1), r.region(2)), (Some((0, 0)), Some((1, 2)), None));
    // a count that disagrees with the bytes is Truncated, one past the limit is LimitExceeded, a stray byte is TrailingBytes
    assert!(matches!(GemCreateExtReq::decode(&out[..n - 4]), Err(WireError::Truncated { .. })));
    assert!(matches!(GemCreateExtReq::decode(&out[..n + 1]), Err(WireError::TrailingBytes { extra: 1 })));
    let many: Vec<(u16, u16)> = (0..=MAX_CREATE_REGIONS).map(|i| (0, i as u16)).collect();
    let mut big = vec![0u8; 1024];
    let m = GemCreateExtReq::encode(&mut big, head, &many).unwrap();
    assert_eq!(GemCreateExtReq::decode(&big[..m]).unwrap_err(), WireError::LimitExceeded { what: "GemCreateExt.region_count", got: 65, max: 64 });
    let mut flagged = out;
    flagged[16 + 8] = 2; // protected = 2 is not a boolean
    assert!(matches!(GemCreateExtReq::decode(&flagged[..n]), Err(WireError::BadValue { .. })));
}

#[test]
fn context_params_flatten_the_engine_map_and_decode_transactionally() {
    let eng: &[(u16, u16)] = &[(0, 0), (1, 0), (0xffff, 0xffff)];
    let mut out = [0u8; 256];
    let n = encode_context_create_ext(&mut out, 2, &[(0xa, 0, 0, eng), (0x9, 0, 1, &[])]).unwrap();
    // head 8 + (24 + 12 padded to 16) + 24
    assert_eq!(n, 8 + 24 + 16 + 24);
    let mut r = ContextCreateExtReq::decode(&out[..n]).unwrap();
    assert_eq!((r.head.flags, r.head.param_count), (2, 2));
    let p0 = r.next_param().unwrap();
    assert_eq!((p0.param, p0.engine_count(), p0.engine(0), p0.engine(2), p0.engine(3)), (0xa, 3, Some((0, 0)), Some((0xffff, 0xffff)), None));
    let p1 = r.next_param().unwrap();
    assert_eq!((p1.param, p1.value, p1.engine_count()), (9, 1, 0));
    assert!(r.next_param().is_none());
    // a record cut short, a stray byte, a non-zero engine pad, and too many engines are all refused whole
    assert!(matches!(ContextCreateExtReq::decode(&out[..n - 1]), Err(WireError::Truncated { .. })));
    assert!(matches!(ContextCreateExtReq::decode(&out[..n + 1]), Err(WireError::TrailingBytes { .. })));
    let mut padded = out;
    padded[8 + 24 + 12] = 1;
    assert!(matches!(ContextCreateExtReq::decode(&padded[..n]), Err(WireError::FieldReservedNonZero { .. })));
    let mut many = [0u8; 512];
    let slots: Vec<(u16, u16)> = (0..65).map(|_| (0, 0)).collect();
    let m = encode_context_create_ext(&mut many, 2, &[(0xa, 0, 0, &slots)]).unwrap();
    assert!(matches!(ContextCreateExtReq::decode(&many[..m]), Err(WireError::LimitExceeded { what: "ctxparam.engine_count", got: 65, max: 64 })));
    // SETPARAM
    let m = ContextSetParamReq::encode(&mut out, 4, 0x6, 0, 1, &[]).unwrap();
    let s = ContextSetParamReq::decode(&out[..m]).unwrap();
    assert_eq!((s.ctx_id, s.param.param, s.param.value), (4, 6, 1));
}

#[test]
fn execbuffer2_requests_round_trip_with_their_tables() {
    let objs = [ExecObject { handle: 1, offset: 0x1000, flags: 0x10, ..Default::default() }, ExecObject { handle: 2, relocation_count: 1, ..Default::default() }];
    let relocs = [Reloc { target_handle: 1, delta: 4, offset: 8, ..Default::default() }];
    let fences = [exec::Fence { handle: 5, flags: 2 }];
    let hdr = exec::ExecHeader { flags: flags::FENCE_ARRAY | flags::NO_RELOC, rsvd1: 3, ..Default::default() };
    let mut out = vec![0u8; 1024];
    let n = exec::encode_request(&mut out, hdr, &objs, &relocs, &fences, &[]).unwrap();
    assert_eq!(n, 64 + 2 * 48 + 32 + 8);
    let h = <exec::ExecHeader as Body>::from_payload(&out[..64]).unwrap();
    assert_eq!((h.buffer_count, h.reloc_total, h.num_cliprects), (2, 1, 1), "the counts come from the tables, never from the caller");
    // a timeline extension block
    let mut w = Writer::new(&mut out[..]);
    put_timeline_fences(&mut w, &[exec::Fence { handle: 7, flags: 1 }], &[9]).unwrap();
    assert_eq!(w.pos(), 24 + 8 + 8 + 8);
    assert!(put_timeline_fences(&mut Writer::new(&mut [0u8; 128]), &[exec::Fence::default()], &[]).is_err());
    // the reply
    let mut rp = vec![0u8; 256];
    let mut w = Writer::new(&mut rp);
    msg::encode_reply_head(&mut w, Status::Ok, 0).unwrap();
    ExecRepHead { request_id: 77, seqno: 2, flags: REP_SUBMITTED, out_fence: 0, n_offsets: 2, n_reloc_updates: 1, ..Default::default() }.encode(&mut w).unwrap();
    w.u64(0x1000).unwrap();
    w.u64(0x2000).unwrap();
    RelocUpdate { index: 0, presumed_offset: 0x1000, ..Default::default() }.encode(&mut w).unwrap();
    let len = w.pos();
    let r = ExecReply::decode(&rp[..len]).unwrap();
    assert_eq!((r.head.request_id, r.head.seqno, r.offset(0), r.offset(1), r.offset(2)), (77, 2, Some(0x1000), Some(0x2000), None));
    assert_eq!(r.update(0), Some(RelocUpdate { index: 0, _pad: Pad32, presumed_offset: 0x1000 }));
    assert!(matches!(ExecReply::decode(&rp[..len - 1]), Err(WireError::Truncated { .. })));
    assert!(matches!(ExecReply::decode(&rp[..len + 1]), Err(WireError::TrailingBytes { .. })));
    assert_eq!((MAX_BUFFERS, MAX_RELOCS, MAX_FENCES), (1024, 1024, 256));
}

#[test]
fn syncobj_arrays_pad_to_eight_and_are_bounded() {
    let mut out = [0u8; 256];
    let n = WaitReq::encode(&mut out, sync::flags::WAIT_ALL | sync::flags::WAIT_FOR_SUBMIT, 5_000, &[1, 2, 3], Some(&[10, 20, 30])).unwrap();
    assert_eq!(n, 16 + 16 + 24, "3 handles pad to 16 bytes, then 3 points");
    let r = WaitReq::decode(&out[..n], true).unwrap();
    assert_eq!((r.head.count, r.head.timeout_ns, r.handles.get(2), r.points.unwrap().get(1)), (3, 5_000, Some(3), Some(20)));
    // the timeline form requires the points, the binary form refuses them
    assert!(matches!(WaitReq::decode(&out[..n - 8], true), Err(WireError::Truncated { .. })));
    assert!(matches!(WaitReq::decode(&out[..n], false), Err(WireError::TrailingBytes { .. })));
    // a non-zero byte in the pad
    let mut bad = out;
    bad[16 + 12] = 9;
    assert!(matches!(WaitReq::decode(&bad[..n], true), Err(WireError::FieldReservedNonZero { .. })));
    // a count past the validation limit is refused before its table is looked at
    let mut head = [0u8; 16];
    head[..4].copy_from_slice(&(MAX_HANDLES + 1).to_le_bytes());
    assert_eq!(WaitReq::decode(&head, false).unwrap_err(), WireError::LimitExceeded { what: "SyncobjWait.count", got: 4097, max: 4096 });
    let m = ArrayReq::encode(&mut out, 0, &[4, 5], Some(&[1, 2])).unwrap();
    let a = ArrayReq::decode(&out[..m], true).unwrap();
    assert_eq!((a.head.count, a.handles.get(1), a.points.unwrap().get(0)), (2, Some(5), Some(1)));
    assert!(ArrayReq::encode(&mut out, 0, &[4, 5], Some(&[1])).is_err(), "points must match handles");
    // the query reply
    let q = sync::encode_query_reply(&mut out, 0, &[3, 5]).unwrap();
    let (e, pts) = sync::decode_query_reply(&out[..q]).unwrap();
    assert_eq!((e, pts.get(0), pts.get(1), pts.len()), (0, Some(3), Some(5), 2));
}

#[test]
fn query_requests_and_replies_walk_their_items() {
    let items = [QueryItem { query_id: 2, length: 0, ..Default::default() }, QueryItem { query_id: 1, length: 64, hdr: [1, 2, 3, 4], flags: 0 }];
    let mut out = [0u8; 256];
    let n = encode_query_request(&mut out, 0, &items).unwrap();
    assert_eq!(n, 8 + 64);
    let (rq, it) = decode_query_request(&out[..n]).unwrap();
    assert_eq!((rq.num_items, it.len(), it.get(1), it.get(2)), (2, 2, Some(items[1]), None));
    assert!(matches!(decode_query_request(&out[..n - 1]), Err(WireError::Truncated { .. })));
    assert!(matches!(decode_query_request(&out[..n + 1]), Err(WireError::TrailingBytes { .. })));
    let too_many = vec![QueryItem::default(); 17];
    assert_eq!(encode_query_request(&mut [0u8; 1024], 0, &too_many).unwrap_err(), WireError::LimitExceeded { what: "query.num_items", got: 17, max: 16 });
    // a reply: head(8) + num_items, pad + item(length 3, data_len 3, "abc" padded to 8) + item(length -19, 0 bytes)
    let mut rp = vec![];
    rp.extend_from_slice(&0u32.to_le_bytes());
    rp.extend_from_slice(&0i32.to_le_bytes());
    rp.extend_from_slice(&2u32.to_le_bytes());
    rp.extend_from_slice(&0u32.to_le_bytes());
    rp.extend_from_slice(&3i32.to_le_bytes());
    rp.extend_from_slice(&3u32.to_le_bytes());
    rp.extend_from_slice(b"abc\0\0\0\0\0");
    rp.extend_from_slice(&(-19i32).to_le_bytes());
    rp.extend_from_slice(&0u32.to_le_bytes());
    let (errno, got) = decode_query_reply(&rp).unwrap();
    assert_eq!(errno, 0);
    assert_eq!(got[0], Some(QueryReplyItem { length: 3, data: b"abc" }));
    assert_eq!(got[1], Some(QueryReplyItem { length: -19, data: b"" }));
    assert_eq!(got[2], None);
}

#[test]
fn replies_carry_a_status_and_a_nonpositive_errno_only() {
    let mut out = [0u8; 64];
    let n = encode_unsupported(&mut out, unsupported::INCREMENT_NOT_ENABLED).unwrap();
    let r = decode_reply::<UnsupportedBody>(&out[..n]).unwrap();
    assert_eq!((r.status, r.errno, r.body.reason), (Status::Unsupported, -errno::EOPNOTSUPP, 2));
    assert_eq!(unsupported::name(2), "INCREMENT-NOT-ENABLED");
    assert_eq!(unsupported::name(1), "NOT-IMPLEMENTED");
    // a positive errno in a reply is a bug on the sender's side: refused by name
    let mut bad = out;
    bad[4..8].copy_from_slice(&5i32.to_le_bytes());
    assert_eq!(decode_reply::<UnsupportedBody>(&bad[..n]).unwrap_err(), WireError::BadValue { what: "reply.errno", got: 5 });
    bad[0..4].copy_from_slice(&99u32.to_le_bytes());
    assert_eq!(decode_reply::<UnsupportedBody>(&bad[..n]).unwrap_err(), WireError::BadValue { what: "reply.status", got: 99 });
    // a Reader refuses what is not there
    let mut rd = Reader::new(&[1, 2, 3]);
    assert!(matches!(rd.u32("x"), Err(WireError::Truncated { need: 4, have: 3, .. })));
}

#[test]
fn errno_numbers_are_linux_x86_64() {
    // include/uapi/asm-generic/errno-base.h and errno.h, written out by hand
    assert_eq!((errno::EPERM, errno::ENOENT, errno::EIO, errno::E2BIG, errno::ENOMEM, errno::EFAULT, errno::EINVAL, errno::ENOSPC), (1, 2, 5, 7, 12, 14, 22, 28));
    assert_eq!((errno::EBADSLT, errno::ETIME, errno::EOPNOTSUPP, errno::ENODEV, errno::EACCES, errno::EBUSY, errno::EAGAIN, errno::EBADF), (57, 62, 95, 19, 13, 16, 11, 9));
    assert_eq!(errno::neg(errno::EINVAL), -22);
}
