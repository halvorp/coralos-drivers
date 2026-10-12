// SPDX-License-Identifier: GPL-2.0-only
//! The capability chain and the multi-link list: found offsets, every named refusal, and the exact reads.
use avs_fw_core::effect::{Op, Space, Width};
use hda_ext_core::caps::*;
use hda_ext_core::error::HdaError;

/// A capability header: version in 31:28, id in 27:16 (AZX_CAP_HDR_*), next pointer in 15:0 (hda_register.h:193-197).
fn hdr(id: u32, next: u32) -> u32 {
    (1 << 28) | (id << 16) | next
}

/// A BAR0 image with the given dword headers and LLCH.
fn bar(llch: u32, hdrs: &[(u32, u32)]) -> Vec<u8> {
    let mut b = vec![0u8; 0x4000];
    b[0x14..0x16].copy_from_slice(&(llch as u16).to_le_bytes());
    for (off, v) in hdrs {
        b[*off as usize..*off as usize + 4].copy_from_slice(&v.to_le_bytes());
    }
    b
}

fn rd(b: &[u8], op: Op) -> u32 {
    match op {
        Op::Read { reg } => {
            assert_eq!(reg.space, Space::Hda);
            let o = reg.off as usize;
            match reg.width {
                Width::W16 => u16::from_le_bytes([b[o], b[o + 1]]) as u32,
                Width::D32 => u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]),
                Width::B8 => b[o] as u32,
            }
        }
        o => panic!("{o:?}"),
    }
}

fn walk(b: &[u8]) -> (Result<Caps, HdaError>, Vec<(u32, Width)>) {
    let mut w = CapWalk::new();
    let mut reads = vec![];
    let mut input = WalkIn::Begin;
    loop {
        match w.next(input) {
            WalkOut::Read(s) => {
                if let Op::Read { reg } = s.op {
                    reads.push((reg.off, reg.width));
                }
                input = WalkIn::Value(rd(b, s.op));
            }
            WalkOut::Done(c) => return (Ok(c), reads),
            WalkOut::Failed(e) => return (Err(e), reads),
        }
    }
}

#[test]
fn the_full_chain_is_found_in_order_with_one_llch_read_and_one_read_per_header() {
    // PP at 0x800 -> SPB at 0x700 -> ML at 0xc00 -> GTS at 0x520 -> DRSM at 0x7c0 -> end
    let b = bar(0x800, &[(0x800, hdr(0x3, 0x700)), (0x700, hdr(0x4, 0xc00)), (0xc00, hdr(0x2, 0x520)), (0x520, hdr(0x1, 0x7c0)), (0x7c0, hdr(0x5, 0))]);
    let (c, reads) = walk(&b);
    assert_eq!(c.unwrap(), Caps { ml: Some(0xc00), gts: Some(0x520), pp: Some(0x800), spb: Some(0x700), drsm: Some(0x7c0) });
    assert_eq!(reads, vec![(0x14, Width::W16), (0x800, Width::D32), (0x700, Width::D32), (0xc00, Width::D32), (0x520, Width::D32), (0x7c0, Width::D32)]);
}

#[test]
fn each_capability_id_lands_in_its_own_slot() {
    // hda_register.h: ML 0x2 (:259), GTS 0x1 (:214), PP 0x3 (:222), SPB 0x4 (:200), DRSM 0x5 (:323)
    for (id, pick) in [(0x2u32, 0usize), (0x1, 1), (0x3, 2), (0x4, 3), (0x5, 4)] {
        let b = bar(0x100, &[(0x100, hdr(id, 0))]);
        let c = walk(&b).0.unwrap();
        let slots = [c.ml, c.gts, c.pp, c.spb, c.drsm];
        for (i, s) in slots.iter().enumerate() {
            assert_eq!(*s, if i == pick { Some(0x100) } else { None }, "id {id:#x} slot {i}");
        }
    }
}

#[test]
fn a_bad_start_a_dead_window_and_an_unknown_id_are_named() {
    assert_eq!(walk(&bar(0, &[])).0, Err(HdaError::BadCapabilityStart { offset: 0 }));
    assert_eq!(walk(&bar(0x802, &[])).0, Err(HdaError::BadCapabilityStart { offset: 0x802 }));
    assert_eq!(walk(&bar(0x800, &[(0x800, 0xFFFF_FFFF)])).0, Err(HdaError::CapabilityReadAllOnes { offset: 0x800 }));
    assert_eq!(walk(&bar(0x800, &[(0x800, hdr(0x9, 0))])).0, Err(HdaError::UnknownCapability { id: 9, offset: 0x800 }));
    // a pointer that is not dword aligned
    assert_eq!(walk(&bar(0x800, &[(0x800, hdr(0x3, 0x702))])).0, Err(HdaError::MisalignedCapability { offset: 0x702 }));
}

#[test]
fn a_chain_that_loops_stops_after_exactly_hdac_max_caps_plus_one_headers() {
    // two headers pointing at each other: Linux would run until counter > 10 (controller.c:466) and return success
    let b = bar(0x800, &[(0x800, hdr(0x3, 0x700)), (0x700, hdr(0x4, 0x800))]);
    let (c, reads) = walk(&b);
    assert_eq!(c, Err(HdaError::TooManyCapabilities { read: 11 }));
    assert_eq!(reads.len(), 1 + 11, "LLCH and eleven headers");
    // a chain of exactly HDAC_MAX_CAPS (10) headers that ends is fine
    let mut hs = vec![];
    for i in 0..10u32 {
        let off = 0x800 + 4 * i;
        hs.push((off, hdr(if i % 2 == 0 { 3 } else { 4 }, if i == 9 { 0 } else { off + 4 })));
    }
    assert!(walk(&bar(0x800, &hs)).0.is_ok());
}

fn ml_bar(count_minus_1: u32, lcaps: &[u32], leptr: &[u32]) -> Vec<u8> {
    let mlcap = 0x800u32;
    let mut b = vec![0u8; 0x4000];
    b[(mlcap + 4) as usize..(mlcap + 8) as usize].copy_from_slice(&count_minus_1.to_le_bytes());
    for (i, l) in lcaps.iter().enumerate() {
        let base = (mlcap + 0x40 + 0x40 * i as u32) as usize;
        b[base..base + 4].copy_from_slice(&l.to_le_bytes());
        b[base + 0x0c..base + 0x0e].copy_from_slice(&(0x0003u16 + i as u16).to_le_bytes());
        b[base + 0x20..base + 0x24].copy_from_slice(&leptr[i].to_le_bytes());
    }
    b
}

fn probe(b: &[u8], bar_len: u32) -> (Result<Vec<Link>, HdaError>, Vec<(u32, Width)>) {
    let mut m = MlProbe::new(0x800, bar_len);
    let mut input = WalkIn::Begin;
    let mut reads = vec![];
    loop {
        match m.next(input) {
            MlOut::Read(s) => {
                if let Op::Read { reg } = s.op {
                    reads.push((reg.off, reg.width));
                }
                input = WalkIn::Value(rd(b, s.op));
            }
            MlOut::Done(l) => return (Ok(l), reads),
            MlOut::Failed(e) => return (Err(e), reads),
        }
    }
}

#[test]
fn the_link_list_reads_count_then_lcap_lsdiid_and_leptr_only_for_alt_links() {
    // link 0: HDA link, not alt, slcount field 0; link 1: alt (bit 28), SLCOUNT = 3 (bits 22:20) -> 4 streams, LEPTR id 0xC0 (SSP)
    let b = ml_bar(1, &[0x0000_0000, (1 << 28) | (3 << 20)], &[0, 0xC000_0000]);
    let (l, reads) = probe(&b, 0x4000);
    let l = l.unwrap();
    assert_eq!(l.len(), 2);
    assert_eq!((l[0].index, l[0].base, l[0].slcount, l[0].id, l[0].alt(), l[0].is_ssp()), (0, 0x840, 1, None, false, false));
    assert_eq!((l[1].index, l[1].base, l[1].slcount, l[1].id, l[1].alt(), l[1].is_ssp()), (1, 0x880, 4, Some(0xC0), true, true));
    assert_eq!(
        reads,
        vec![(0x804, Width::D32), (0x840, Width::D32), (0x84c, Width::W16), (0x880, Width::D32), (0x88c, Width::W16), (0x8a0, Width::D32)]
    );
}

#[test]
fn the_leptr_id_values_are_the_linux_ones() {
    // hda_register.h:315-318: SDW 0x00, SSP 0xC0, DMIC 0xC1, UAOL 0xC2 - only 0xC0 is the SSP link
    for (id, ssp) in [(0x00u32, false), (0xC0, true), (0xC1, false), (0xC2, false)] {
        let b = ml_bar(0, &[1 << 28], &[id << 24]);
        let l = probe(&b, 0x4000).0.unwrap();
        assert_eq!(l[0].id, Some(id));
        assert_eq!(l[0].is_ssp(), ssp, "{id:#x}");
    }
}

#[test]
fn a_link_count_that_cannot_fit_the_bar_is_refused_before_anything_is_reserved() {
    // 0xFFFFFFFF + 1 wraps to 0 links; 1000 links do not fit a 16 KiB BAR; the exact fit is fine
    assert!(matches!(probe(&ml_bar(0xFFFF_FFFF, &[], &[]), 0x4000).0, Err(HdaError::LinkWindowPastBar { links: 0, .. })));
    assert!(matches!(probe(&ml_bar(999, &[], &[]), 0x4000).0, Err(HdaError::LinkWindowPastBar { links: 1000, mlcap: 0x800, bar_len: 0x4000 })));
    // mlcap 0x800 + 0x40 + n*0x40 <= 0x4000  =>  n <= (0x4000 - 0x840) / 0x40 = 0xDF
    let fits = 0xDF;
    let lcaps = vec![0u32; fits];
    let lep = vec![0u32; fits];
    assert_eq!(probe(&ml_bar(fits as u32 - 1, &lcaps, &lep), 0x4000).0.unwrap().len(), fits);
    assert!(matches!(probe(&ml_bar(fits as u32, &[], &[]), 0x4000).0, Err(HdaError::LinkWindowPastBar { .. })));
}
