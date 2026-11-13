// SPDX-License-Identifier: GPL-2.0-only
//! Cherryview (8086:22B0, the Z83's GT, DECISIONS.md D8) through the render bring-up on a simulated chip: the executor on Cherryview's own
//! allow-list and forcewake domains (FORCEWAKE_VLV / FORCEWAKE_MEDIA_VLV, intel_gt_regs.h:1513-1516), the CHV GGTT size
//! (`chv_get_total_gtt_size`), the CHV private PAT (`chv_setup_private_ppat`, gt/intel_gtt.c:655-662), no MOCS word (Cherryview has
//! no MOCS table, gt/intel_mocs.c:454-534), and the render engine up: `[I915-UAPI READY engines=rcs0 ...]`.
#![cfg(feature = "p3-exec")]
mod exec_common;
use coralos_hal::fake::FakeDma;
use exec_common::{Chip, Dom, Mem, ENDOWMENT, PHYS_BASE};
use i915_device::exec::dma::{GpuAlloc, GpuMem};
use i915_device::exec::Executor;
use i915_device::uapi::{bring_up_render, Hw, RenderConfig};
use i915_devinfo_core::{lookup, PlatformDescriptor};
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;

fn chv() -> PlatformDescriptor {
    lookup(0x22b0).expect("CHV row")
}

/// The simulated chip with Cherryview's two forcewake domains in place of Gen9's (the middle slot is an unreachable placeholder: no
/// list grants a write to it).
fn chv_chip() -> Chip {
    let chip = Chip::new();
    {
        let mut s = chip.0.borrow_mut();
        s.doms[0] = Dom { set: 0x1300b0, ack: 0x1300b4, state: 0, ack_val: 0 }; // FORCEWAKE_VLV / _ACK_VLV
        s.doms[1] = Dom { set: 0x1, ack: 0x5, state: 0, ack_val: 0 };
        s.doms[2] = Dom { set: 0x1300b8, ack: 0x1300bc, state: 0, ack_val: 0 }; // FORCEWAKE_MEDIA_VLV / _ACK_MEDIA_VLV
    }
    chip
}

#[test]
fn cherryview_brings_up_the_render_engine_on_its_own_tables() {
    let chip = chv_chip();
    let mem = Mem::default();
    let mut gm = GpuMem::new(GpuAlloc::new(FakeDma::new(PHYS_BASE), &chv(), ENDOWMENT), mem);
    let mut ex = Executor::new(chip.clone(), &chv()).expect("Cherryview's executor");
    let mut log = exec_common::Lines::default();
    let dev = UapiDevice::new(chv(), 0x21, i915_devinfo_core::sseu::chv_sseu_info(0), i915_devinfo_core::clock::GEN6_TS_HZ, 4 << 30).unwrap();
    // SNB GGMS = 3 (bits 9:8): an 8 MiB table, a 4 GiB GGTT, 16 fences
    let geom = GgttGeometry::probe_for(&chv(), 3 << 8, 16 << 20, 256 << 20).unwrap();
    assert_eq!((geom.total, geom.fences), (4 << 30, 16));
    let cfg = RenderConfig { quirks: i915_device::gt::flush_quirks(&chv(), 0x21), stall_bound_ms: 1000, keep: None };
    let _svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, cfg).expect("render up on Cherryview");

    assert_eq!(log.find("[I915-UAPI READY engines=rcs0").len(), 1, "{:#?}", log.0);
    // chv_setup_private_ppat: SNOOP (bit 6) in entries 0 and 4..7
    assert_eq!(chip.writes_to(0x40e0), [0x0000_0040]);
    assert_eq!(chip.writes_to(0x40e4), [0x4040_4040]);
    let writes = chip.all_writes();
    // no MOCS word at all: GEN9_GFX_MOCS (0xc800..), the engine blocks (0xc900, 0xcb00, 0xcc00), GEN9_LNCFCMOCS (0xb020..0xb09f)
    assert!(!writes.iter().any(|(r, _)| (0xc800..0xcd00).contains(r) || (0xb020..0xb0a0).contains(r)), "{writes:x?}");
    // forcewake only through the VLV domains; nothing was refused by the guard
    let fw = chip.0.borrow().fw_writes.clone();
    assert!(!fw.is_empty() && fw.iter().all(|(r, _)| *r == 0x1300b0 || *r == 0x1300b8), "{fw:x?}");
    assert_eq!(ex.stats().refused, 0);
}
