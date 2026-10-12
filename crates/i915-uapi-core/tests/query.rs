// SPDX-License-Identifier: GPL-2.0-only
//! I915_QUERY: byte-exact vectors derived by hand from i915_query.c / intel_sseu.c (v7.0-rc6) for the 24-EU Kaby Lake GT2 fuse state
//! (1 slice, subslice 3 fused off, no EU fused: 3 x 8 = 24 EUs) and a 48-EU GT3, plus the protocol rules.
mod common;
use common::*;
use i915_uapi_core::codec::Writer;
use i915_uapi_core::consts::query::*;
use i915_uapi_core::query::{handle_query, query_item, ItemResult, MAX_QUERY_ITEMS};
use i915_uapi_core::types::QueryItem;
use i915_uapi_core::Errno;

fn item(id: u32, length: i32) -> QueryItem {
    QueryItem { query_id: u64::from(id), length, flags: 0, hdr: [0; 4] }
}

fn run(d: &i915_uapi_core::UapiDevice, it: &QueryItem) -> (ItemResult, Vec<u8>) {
    let mut buf = vec![0xAAu8; 512];
    let r = query_item(d, it, &mut buf);
    buf.truncate(r.written);
    (r, buf)
}

#[test]
fn topology_bytes_for_the_24_eu_gt2() {
    // fill_topology_info (i915_query.c:34-89) with max_slices=3, max_subslices=4, max_eus=8 (intel_sseu.c:390-391):
    // slice_length = 1, ss_stride = 1 -> subslice_length = 3, eu_stride = 1 -> eu_length = 3*4*1 = 12, total = 16+1+3+12 = 32
    let (r, b) = run(&gt2(), &item(DRM_I915_QUERY_TOPOLOGY_INFO, 32));
    assert_eq!((r.length, r.written), (32, 32));
    #[rustfmt::skip]
    let want: [u8; 32] = [
        // drm_i915_query_topology_info: flags, max_slices, max_subslices, max_eus_per_subslice, subslice_offset,
        //                               subslice_stride, eu_offset, eu_stride (all u16 LE)
        0x00, 0x00,  0x03, 0x00,  0x04, 0x00,  0x08, 0x00,  0x01, 0x00,  0x01, 0x00,  0x04, 0x00,  0x01, 0x00,
        // slice_mask: slice 0 only
        0x01,
        // subslice mask per slice: slice 0 has subslices 0..2
        0x07, 0x00, 0x00,
        // EU mask per (slice, subslice): slice 0 ss0..2 all 8 EUs, ss3 absent; slices 1 and 2 absent
        0xff, 0xff, 0xff, 0x00,
        0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(b, want);
}

#[test]
fn topology_bytes_for_the_48_eu_gt3_with_one_fused_eu() {
    // slices 0 and 1, subslice 3 fused off in both, EU 0 of slice 1 / subslice 2 fused (eu_disable[1] bit 16)
    let d = kbl(0x5926, fuse2(0b011, 0b1000), [0, 1 << 16, 0]);
    let (r, b) = run(&d, &item(DRM_I915_QUERY_TOPOLOGY_INFO, 32));
    assert_eq!(r.length, 32);
    assert_eq!(&b[16..17], &[0x03], "slice_mask");
    assert_eq!(&b[17..20], &[0x07, 0x07, 0x00], "one subslice byte per slice (bit s*8+ss)");
    assert_eq!(&b[20..32], &[0xff, 0xff, 0xff, 0x00, 0xff, 0xff, 0xfe, 0x00, 0, 0, 0, 0], "EU masks; slice 1 ss2 = 0xfe");
}

#[test]
fn query_size_protocol() {
    let d = gt2();
    // length 0 -> the size needed, nothing written (copy_query_item i915_query.c:21-22)
    let (r, b) = run(&d, &item(DRM_I915_QUERY_TOPOLOGY_INFO, 0));
    assert_eq!((r.length, r.written, b.len()), (32, 0, 0));
    // 0 < length < total -> -EINVAL (:24-25), nothing written
    let (r, _) = run(&d, &item(DRM_I915_QUERY_TOPOLOGY_INFO, 31));
    assert_eq!((r.length, r.written), (-22, 0));
    // length > total is fine; the produced size is returned
    let (r, b) = run(&d, &item(DRM_I915_QUERY_TOPOLOGY_INFO, 4096));
    assert_eq!((r.length, b.len()), (32, 32));
    // i915_query.c:24 compares an s32 with a u32: a NEGATIVE length converts to a huge unsigned and passes. Faithful.
    let (r, b) = run(&d, &item(DRM_I915_QUERY_TOPOLOGY_INFO, -1));
    assert_eq!((r.length, b.len()), (32, 32));
    // a reply buffer smaller than the data is the copy_to_user fault, -EFAULT, with nothing counted as written
    let mut tiny = [0u8; 8];
    let r = query_item(&d, &item(DRM_I915_QUERY_TOPOLOGY_INFO, 32), &mut tiny);
    assert_eq!((r.length, r.written), (Errno::EFAULT.neg(), 0));
}

#[test]
fn flags_must_be_zero_and_unknown_ids_are_einval() {
    let d = gt2();
    for id in [DRM_I915_QUERY_TOPOLOGY_INFO, DRM_I915_QUERY_ENGINE_INFO, DRM_I915_QUERY_MEMORY_REGIONS] {
        let mut it = item(id, 0);
        it.flags = 1;
        let (r, _) = run(&d, &it);
        assert_eq!(r.length, -22, "query {id} with flags=1 (i915_query.c:97,142,475)");
    }
    let (r, _) = run(&d, &item(8, 0)); // one past the table (:624-630)
    assert_eq!(r.length, -22);
    let mut big = item(1, 0);
    big.query_id = 1 << 40; // a huge id is still just "unknown"
    let (r, _) = run(&d, &big);
    assert_eq!(r.length, -22);
}

#[test]
fn engine_info_for_gt2_and_gt3() {
    let d = gt2();
    let (r, b) = run(&d, &item(DRM_I915_QUERY_ENGINE_INFO, 0));
    assert_eq!(r.length, 16 + 4 * 56); // struct_size(query, engines, 4) (:148)
    let (r, b2) = run(&d, &item(DRM_I915_QUERY_ENGINE_INFO, 240));
    assert_eq!((r.length, b2.len()), (240, 240));
    assert!(b.is_empty());
    // header: num_engines = 4, rsvd[3] = 0
    assert_eq!(&b2[..16], &[4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    // each drm_i915_engine_info: class u16, instance u16, rsvd0 u32, flags u64 (=1), capabilities u64, logical_instance u16, pad
    let eng = |class: u8, instance: u8, caps: u8, logical: u8| {
        let mut e = [0u8; 56];
        e[0] = class;
        e[2] = instance;
        e[8] = 1; // I915_ENGINE_INFO_HAS_LOGICAL_INSTANCE
        e[16] = caps;
        e[24] = logical;
        e
    };
    // uabi order: RCS0, BCS0, VCS0 (HEVC|SFC = 3), VECS0 (gt/intel_engine_user.c:55-73; capabilities intel_engine_cs.c:627-641)
    assert_eq!(&b2[16..72], &eng(0, 0, 0, 0));
    assert_eq!(&b2[72..128], &eng(1, 0, 0, 0));
    assert_eq!(&b2[128..184], &eng(2, 0, 3, 0));
    assert_eq!(&b2[184..240], &eng(3, 0, 0, 0));

    // GT3: a second VDBOX with logical_instance 1 and NO capabilities (instance != 0 below Gen11)
    let g3 = gt3();
    let (r, b3) = run(&g3, &item(DRM_I915_QUERY_ENGINE_INFO, 296));
    assert_eq!((r.length, b3.len()), (296, 296));
    assert_eq!(b3[0], 5);
    assert_eq!(&b3[128..184], &eng(2, 0, 3, 0));
    assert_eq!(&b3[184..240], &eng(2, 1, 0, 1));
    assert_eq!(&b3[240..296], &eng(3, 0, 0, 0));
}

#[test]
fn engine_info_and_memory_regions_demand_a_zero_header() {
    // i915_query.c:154-156 (num_engines, rsvd[0..3]) and :490-495 (num_regions, rsvd[0..3])
    for id in [DRM_I915_QUERY_ENGINE_INFO, DRM_I915_QUERY_MEMORY_REGIONS] {
        for slot in 0..4 {
            let mut it = item(id, 4096);
            it.hdr[slot] = 1;
            let (r, b) = run(&gt2(), &it);
            assert_eq!((r.length, b.len()), (-22, 0), "query {id} header word {slot}");
        }
    }
}

#[test]
fn memory_regions_list_system_only() {
    let mut d = gt2();
    d.system_memory_bytes = 0x2_0000_0000; // 8 GiB
    d.system_available_bytes = 0x1_8000_0000;
    let (r, b) = run(&d, &item(DRM_I915_QUERY_MEMORY_REGIONS, 0));
    assert_eq!((r.length, b.len()), (16 + 88, 0)); // header + one region (:479-484)
    let (r, b) = run(&d, &item(DRM_I915_QUERY_MEMORY_REGIONS, 104));
    assert_eq!((r.length, b.len()), (104, 104));
    // header: num_regions = 1, rsvd[3]
    assert_eq!(&b[..16], &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    // region: class SYSTEM (0), instance 0, rsvd0, probed_size, unallocated_size (== probed for an unprivileged caller, :515-518),
    // probed_cpu_visible_size (== total for a non-LOCAL region, :506-509), unallocated_cpu_visible_size
    let g8 = 0x2_0000_0000u64.to_le_bytes();
    assert_eq!(&b[16..24], &[0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&b[24..32], &g8);
    assert_eq!(&b[32..40], &g8);
    assert_eq!(&b[40..48], &g8);
    assert_eq!(&b[48..56], &g8);
    assert!(b[56..104].iter().all(|x| *x == 0));
    // perfmon_capable() sees the live availability (:511-519)
    d.perfmon_capable = true;
    let (_, b) = run(&d, &item(DRM_I915_QUERY_MEMORY_REGIONS, 104));
    assert_eq!(&b[32..40], &0x1_8000_0000u64.to_le_bytes());
    assert_eq!(&b[24..32], &g8, "probed size never changes");
}

#[test]
fn queries_a_gen9_execlists_device_cannot_answer() {
    let d = gt2();
    for id in [DRM_I915_QUERY_HWCONFIG_BLOB, DRM_I915_QUERY_GEOMETRY_SUBSLICES, DRM_I915_QUERY_GUC_SUBMISSION_VERSION] {
        let (r, _) = run(&d, &item(id, 0));
        assert_eq!(r.length, -19, "query {id} -> -ENODEV");
    }
    for f in [DRM_I915_QUERY_PERF_CONFIG_LIST, DRM_I915_QUERY_PERF_CONFIG_DATA_FOR_UUID, DRM_I915_QUERY_PERF_CONFIG_DATA_FOR_ID] {
        let mut it = item(DRM_I915_QUERY_PERF_CONFIG, 0);
        it.flags = f;
        assert_eq!(run(&d, &it).0.length, -19, "perf config flags {f}");
    }
    let mut it = item(DRM_I915_QUERY_PERF_CONFIG, 0);
    it.flags = 4;
    assert_eq!(run(&d, &it).0.length, -22);
}

fn req(num_items: u32, flags: u32, items: &[QueryItem]) -> Vec<u8> {
    let mut b = vec![0u8; 64 + items.len() * 32];
    let n = {
        let mut w = Writer::new(&mut b);
        w.u32(num_items).unwrap();
        w.u32(flags).unwrap();
        for i in items {
            i.encode(&mut w).unwrap();
        }
        w.pos()
    };
    b.truncate(n);
    b
}

#[test]
fn wire_round_trip_two_items() {
    let d = gt2();
    let r = req(2, 0, &[item(DRM_I915_QUERY_TOPOLOGY_INFO, 0), item(DRM_I915_QUERY_TOPOLOGY_INFO, 32)]);
    let mut out = vec![0u8; 512];
    let n = handle_query(&d, &r, &mut out).unwrap();
    // reply header: status Ok, errno 0, num_items 2, pad
    assert_eq!(&out[..16], &[0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
    // item 0: length 32, no data; item 1: length 32, 32 data bytes
    assert_eq!(&out[16..24], &[32, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&out[24..32], &[32, 0, 0, 0, 32, 0, 0, 0]);
    assert_eq!(&out[32..48], &[0, 0, 3, 0, 4, 0, 8, 0, 1, 0, 1, 0, 4, 0, 1, 0]);
    assert_eq!(n, 32 + 32);
}

#[test]
fn whole_call_refusals_are_transactional() {
    let d = gt2();
    let mut out = vec![0u8; 512];
    // drm_i915_query::flags != 0 -> -EINVAL (i915_query.c:607-608): no per-item results
    let n = handle_query(&d, &req(1, 1, &[item(1, 32)]), &mut out).unwrap();
    assert_eq!(n, 16);
    assert_eq!(&out[..16], &[0, 0, 0, 0, 0xea, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0]);
    // item.query_id == 0 -> -EINVAL for the whole call (:618-619), even after a good item
    let n = handle_query(&d, &req(2, 0, &[item(1, 32), item(0, 0)]), &mut out).unwrap();
    assert_eq!((n, out[4]), (16, 0xea));
}

#[test]
fn wire_bounds() {
    let d = gt2();
    let mut out = vec![0u8; 4096];
    // num_items above the port bound
    let e = handle_query(&d, &req(MAX_QUERY_ITEMS + 1, 0, &[]), &mut out).unwrap_err();
    assert_eq!((e.name(), e.errno()), ("LIMIT-EXCEEDED", Errno::ENOMEM));
    // exactly the bound is accepted
    let items: Vec<QueryItem> = (0..MAX_QUERY_ITEMS).map(|_| item(1, 0)).collect();
    assert!(handle_query(&d, &req(MAX_QUERY_ITEMS, 0, &items), &mut out).is_ok());
    // declared items missing from the payload
    assert_eq!(handle_query(&d, &req(2, 0, &[item(1, 0)]), &mut out).unwrap_err().name(), "WIRE-TRUNCATED");
    // extra bytes after the last item
    let mut r = req(1, 0, &[item(1, 0)]);
    r.push(0);
    assert_eq!(handle_query(&d, &r, &mut out).unwrap_err().name(), "WIRE-TRAILING-BYTES");
    // a reply buffer that cannot hold the answer is refused, never truncated
    let mut small = vec![0u8; 40];
    assert_eq!(handle_query(&d, &req(1, 0, &[item(1, 32)]), &mut small).unwrap_err().name(), "QUERY-REPLY-TOO-SMALL");
}
