// SPDX-License-Identifier: GPL-2.0-only
//! Struct sizes and wire encodings, from the header's field lists (i915_drm.h:1118,1239,1327,3057,3167,3345,3482).
use i915_uapi_core::codec::Writer;

use i915_uapi_core::types::*;

fn enc<F: FnOnce(&mut Writer<'_>)>(f: F) -> Vec<u8> {
    let mut b = vec![0u8; 256];
    let n = {
        let mut w = Writer::new(&mut b);
        f(&mut w);
        w.pos()
    };
    b.truncate(n);
    b
}

#[test]
fn wire_sizes_follow_the_header_field_lists() {
    // exec_object2: u32 handle, u32 relocation_count, u64 relocs_ptr, u64 alignment, u64 offset, u64 flags, u64 rsvd1, u64 rsvd2 = 56;
    // flattened without relocs_ptr = 48.
    assert_eq!(ExecObject2::WIRE_BYTES, 56 - 8);
    // relocation_entry: u32 target_handle, u32 delta, u64 offset, u64 presumed_offset, u32 read_domains, u32 write_domain = 32.
    assert_eq!(RelocationEntry::WIRE_BYTES, 4 + 4 + 8 + 8 + 4 + 4);
    assert_eq!(ExecFence::WIRE_BYTES, 4 + 4);
    // i915_user_extension: u64 next_extension, u32 name, u32 flags, u32 rsvd[4] = 32; flattened without next_extension = 24.
    assert_eq!(ExtensionHeader::WIRE_BYTES, 32 - 8);
    // query_item: u64 query_id, s32 length, u32 flags, u64 data_ptr = 24; flattened: data_ptr -> 16-byte header snapshot = 32.
    assert_eq!(QueryItem::WIRE_BYTES, 8 + 4 + 4 + 16);
    // topology_info: 8 x u16
    assert_eq!(TopologyInfoHeader::WIRE_BYTES, 8 * 2);
    // engine_info: class u16, instance u16, rsvd0 u32, flags u64, capabilities u64, logical_instance u16, rsvd1[3] u16, rsvd2[3] u64
    assert_eq!(EngineInfo::WIRE_BYTES, 2 + 2 + 4 + 8 + 8 + 2 + 6 + 24);
    // memory_region_info: class u16, instance u16, rsvd0 u32, probed u64, unallocated u64, union of 8 x u64
    assert_eq!(MemoryRegionInfo::WIRE_BYTES, 2 + 2 + 4 + 8 + 8 + 64);
    assert_eq!(QUERY_LIST_HEADER_BYTES, 4 + 3 * 4);
}

#[test]
fn encoders_write_exactly_wire_bytes_and_decoders_round_trip() {
    let o = ExecObject2 { handle: 7, relocation_count: 2, alignment: 0x10000, offset: 0x1234_5000, flags: 0x58, pad_to_size: 0x2000, rsvd2: 9 };
    let b = enc(|w| o.encode(w).unwrap());
    assert_eq!(b.len(), ExecObject2::WIRE_BYTES);
    assert_eq!(&b[0..8], &[7, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(ExecObject2::decode(b.as_slice().try_into().unwrap()), o);

    let r = RelocationEntry { target_handle: 3, delta: 4, offset: 0x40, presumed_offset: 0xffff_8000_0000_1000, read_domains: 2, write_domain: 2 };
    let b = enc(|w| r.encode(w).unwrap());
    assert_eq!(b.len(), RelocationEntry::WIRE_BYTES);
    assert_eq!(RelocationEntry::decode(b.as_slice().try_into().unwrap()), r);

    let f = ExecFence { handle: 5, flags: 3 };
    let b = enc(|w| f.encode(w).unwrap());
    assert_eq!((b.len(), ExecFence::decode(b.as_slice().try_into().unwrap())), (8, f));

    let q = QueryItem { query_id: 4, length: -22, flags: 0, hdr: [1, 2, 3, 4] };
    let b = enc(|w| q.encode(w).unwrap());
    assert_eq!(b.len(), QueryItem::WIRE_BYTES);
    assert_eq!(QueryItem::decode(b.as_slice().try_into().unwrap()), q);

    let e = ExtensionHeader { name: 0, flags: 1, rsvd: [1, 2, 3, 4] };
    let b = enc(|w| e.encode(w).unwrap());
    assert_eq!(b.len(), ExtensionHeader::WIRE_BYTES);
    assert_eq!(ExtensionHeader::decode(b.as_slice().try_into().unwrap()), e);

    let t = TopologyInfoHeader { flags: 0, max_slices: 3, max_subslices: 4, max_eus_per_subslice: 8, subslice_offset: 1, subslice_stride: 1, eu_offset: 4, eu_stride: 1 };
    let b = enc(|w| t.encode(w).unwrap());
    assert_eq!(b.len(), TopologyInfoHeader::WIRE_BYTES);
    assert_eq!(TopologyInfoHeader::decode(b.as_slice().try_into().unwrap()), t);

    assert_eq!(enc(|w| EngineInfo::default().encode(w).unwrap()).len(), EngineInfo::WIRE_BYTES);
    assert_eq!(enc(|w| MemoryRegionInfo::default().encode(w).unwrap()).len(), MemoryRegionInfo::WIRE_BYTES);
}

#[test]
fn a_full_writer_refuses_by_name_and_writes_nothing_past_the_end() {
    let mut small = [0u8; 4];
    let mut w = Writer::new(&mut small);
    assert!(w.u32(1).is_ok());
    let e = w.u32(2).unwrap_err();
    assert_eq!(e.name(), "QUERY-REPLY-TOO-SMALL");
    assert_eq!(small, [1, 0, 0, 0]);
}

#[test]
fn engine_and_memory_class_values() {
    assert_eq!((engine_class::RENDER, engine_class::COPY, engine_class::VIDEO, engine_class::VIDEO_ENHANCE, engine_class::COMPUTE), (0, 1, 2, 3, 4));
    assert_eq!(engine_class::INVALID, 0xffff);
    assert_eq!((memory_class::SYSTEM, memory_class::DEVICE), (0, 1));
}

#[test]
fn plain_gem_struct_sizes_match_the_c_headers() {
    // sizes computed from the field lists: create{u64,u32,u32}=16; create_ext{u64,u32,u32,u64}=24; mmap_offset{u32,u32,u64,u64,u64}=32;
    // mmap{u32,u32,u64,u64,u64,u64}=40; set_domain 3xu32=12; set/get_tiling 4xu32=16; madvise 3xu32=12; caching 8; busy 8;
    // wait{u32,u32,s64}=16; context_create/destroy 8; context_create_ext{u32,u32,u64}=16; context_param{u32,u32,u64,u64}=24;
    // vm_control{u64,u32,u32}=16; reset_stats 6xu32=24; get_aperture 2xu64=16; reg_read 2xu64=16.
    use core::mem::size_of;
    use i915_uapi_core::gem_types::*;
    assert_eq!(sizeof::GEM_CREATE, 8 + 4 + 4);
    assert_eq!(sizeof::GEM_CREATE_EXT, 8 + 4 + 4 + 8);
    assert_eq!(sizeof::GEM_MMAP_OFFSET, 4 + 4 + 8 + 8 + 8);
    assert_eq!(sizeof::GEM_MMAP, 4 + 4 + 8 + 8 + 8 + 8);
    assert_eq!(sizeof::GEM_SET_DOMAIN, 12);
    assert_eq!(sizeof::GEM_SET_TILING, 16);
    assert_eq!(sizeof::GEM_GET_TILING, 16);
    assert_eq!(sizeof::GEM_MADVISE, 12);
    assert_eq!(sizeof::GEM_CACHING, 8);
    assert_eq!(sizeof::GEM_BUSY, 8);
    assert_eq!(sizeof::GEM_WAIT, 16);
    assert_eq!(sizeof::GEM_CONTEXT_ID, 8);
    assert_eq!(sizeof::GEM_CONTEXT_CREATE_EXT, 16);
    assert_eq!(sizeof::GEM_CONTEXT_PARAM, 24);
    assert_eq!(sizeof::GEM_VM_CONTROL, 16);
    assert_eq!(sizeof::RESET_STATS, 24);
    assert_eq!(sizeof::GEM_GET_APERTURE, 16);
    assert_eq!(sizeof::REG_READ, 16);
    // Rust's own layout of the same field lists agrees (all fields are naturally aligned in the C originals).
    assert_eq!(size_of::<GemCreate>(), sizeof::GEM_CREATE);
    assert_eq!(size_of::<GemCreateExt>(), 16); // extensions pointer is flattened away: 24 - 8
    assert_eq!(size_of::<GemMmapOffset>(), sizeof::GEM_MMAP_OFFSET);
    assert_eq!(size_of::<GemMmap>(), sizeof::GEM_MMAP);
    assert_eq!(size_of::<GemSetDomain>(), sizeof::GEM_SET_DOMAIN);
    assert_eq!(size_of::<GemSetTiling>(), sizeof::GEM_SET_TILING);
    assert_eq!(size_of::<GemGetTiling>(), sizeof::GEM_GET_TILING);
    assert_eq!(size_of::<GemMadvise>(), sizeof::GEM_MADVISE);
    assert_eq!(size_of::<GemCaching>(), sizeof::GEM_CACHING);
    assert_eq!(size_of::<GemBusy>(), sizeof::GEM_BUSY);
    assert_eq!(size_of::<GemWait>(), sizeof::GEM_WAIT);
    assert_eq!(size_of::<GemContextId>(), sizeof::GEM_CONTEXT_ID);
    assert_eq!(size_of::<GemContextCreateExt>(), 8); // extensions flattened away
    assert_eq!(size_of::<GemContextParam>(), sizeof::GEM_CONTEXT_PARAM);
    assert_eq!(size_of::<GemVmControl>(), sizeof::GEM_VM_CONTROL);
    assert_eq!(size_of::<ResetStats>(), sizeof::RESET_STATS);
    assert_eq!(size_of::<GemGetAperture>(), sizeof::GEM_GET_APERTURE);
    assert_eq!(size_of::<RegRead>(), sizeof::REG_READ);
}
