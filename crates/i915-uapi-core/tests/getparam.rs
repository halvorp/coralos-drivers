// SPDX-License-Identifier: GPL-2.0-only
//! GETPARAM: the answers Linux gives for a Kaby Lake GT2 (0x591C) device, written out by hand from i915_getparam.c (v7.0-rc6),
//! and the way each answer MOVES when the device fact it is derived from moves (proof that nothing is hard-coded).
mod common;
use common::*;
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_uapi_core::consts::param::*;
use i915_uapi_core::getparam::{decode_getparam_request, encode_getparam_reply, handle_getparam};
use i915_uapi_core::{getparam, Errno, UapiDevice, UapiError};

fn ok(dev: &UapiDevice, p: u32) -> i32 {
    getparam(dev, p).unwrap_or_else(|e| panic!("param {p}: {e:?}"))
}
fn err(dev: &UapiDevice, p: u32) -> UapiError {
    getparam(dev, p).unwrap_err()
}

#[test]
fn linux_values_for_kbl_gt2_0x591c() {
    let d = gt2();
    assert_eq!(ok(&d, I915_PARAM_CHIPSET_ID), 0x591c); // i915_getparam.c:35 pdev->device
    assert_eq!(ok(&d, I915_PARAM_REVISION), 9); // :38 pdev->revision (the revision handed in)
    assert_eq!(ok(&d, I915_PARAM_NUM_FENCES_AVAIL), 32); // :41, intel_ggtt_fencing.c:855-857
    assert_eq!(ok(&d, I915_PARAM_HAS_OVERLAY), 0); // :44
    assert_eq!(ok(&d, I915_PARAM_HAS_BSD), 1); // :47 VCS0
    assert_eq!(ok(&d, I915_PARAM_HAS_BLT), 1); // :51 BCS0
    assert_eq!(ok(&d, I915_PARAM_HAS_VEBOX), 1); // :55 VECS0
    assert_eq!(ok(&d, I915_PARAM_HAS_BSD2), 0); // :59 VCS1: GT2 has one VDBOX
    assert_eq!(ok(&d, I915_PARAM_HAS_LLC), 1); // :63
    assert_eq!(ok(&d, I915_PARAM_HAS_WT), 0); // :66 no eDRAM on a GT2 Y part
    assert_eq!(ok(&d, I915_PARAM_HAS_ALIASING_PPGTT), 2); // :69 INTEL_PPGTT_FULL
    assert_eq!(ok(&d, I915_PARAM_HAS_SEMAPHORES), 1); // :72
    assert_eq!(ok(&d, I915_PARAM_HAS_SECURE_BATCHES), 0); // :75
    assert_eq!(ok(&d, I915_PARAM_CMD_PARSER_VERSION), 10); // :78 Gen9 BCS parser, i915_cmd_parser.c:1621
    assert_eq!(ok(&d, I915_PARAM_SUBSLICE_TOTAL), 3); // :81
    assert_eq!(ok(&d, I915_PARAM_EU_TOTAL), 24); // :86
    assert_eq!(ok(&d, I915_PARAM_HAS_GPU_RESET), 2); // :91-94 hangcheck + full reset, upgraded by engine reset
    assert_eq!(ok(&d, I915_PARAM_HAS_RESOURCE_STREAMER), 0); // :97
    assert_eq!(ok(&d, I915_PARAM_HAS_POOLED_EU), 0); // :100
    assert_eq!(ok(&d, I915_PARAM_MIN_EU_IN_POOL), 0); // :103
    assert_eq!(ok(&d, I915_PARAM_MMAP_GTT_VERSION), 5); // :124, i915_gem_mman.c:196
    assert_eq!(ok(&d, I915_PARAM_HAS_SCHEDULER), 0x1f); // :127: ENABLED|PRIORITY|PREEMPTION|SEMAPHORES|ENGINE_BUSY_STATS
    assert_eq!(ok(&d, I915_PARAM_MMAP_VERSION), 1); // :130-159
    assert_eq!(ok(&d, I915_PARAM_HAS_CONTEXT_ISOLATION), 0b1111); // :168: classes RENDER|COPY|VIDEO|VIDEO_ENHANCE have a default state
    assert_eq!(ok(&d, I915_PARAM_SLICE_MASK), 1); // :175
    assert_eq!(ok(&d, I915_PARAM_SUBSLICE_MASK), 0b0111); // :185
    assert_eq!(ok(&d, I915_PARAM_CS_TIMESTAMP_FREQUENCY), 12_000_000); // :190: 24 MHz crystal >> (3 - 2)
    assert_eq!(ok(&d, I915_PARAM_MMAP_GTT_COHERENT), 1); // :193 has_coherent_ggtt
}

#[test]
fn the_always_true_feature_bits_are_one() {
    let d = gt2();
    for (name, p) in [
        ("HAS_GEM", I915_PARAM_HAS_GEM),
        ("HAS_PAGEFLIPPING", I915_PARAM_HAS_PAGEFLIPPING),
        ("HAS_EXECBUF2", I915_PARAM_HAS_EXECBUF2),
        ("HAS_RELAXED_FENCING", I915_PARAM_HAS_RELAXED_FENCING),
        ("HAS_COHERENT_RINGS", I915_PARAM_HAS_COHERENT_RINGS),
        ("HAS_RELAXED_DELTA", I915_PARAM_HAS_RELAXED_DELTA),
        ("HAS_GEN7_SOL_RESET", I915_PARAM_HAS_GEN7_SOL_RESET),
        ("HAS_WAIT_TIMEOUT", I915_PARAM_HAS_WAIT_TIMEOUT),
        ("HAS_PRIME_VMAP_FLUSH", I915_PARAM_HAS_PRIME_VMAP_FLUSH),
        ("HAS_PINNED_BATCHES", I915_PARAM_HAS_PINNED_BATCHES),
        ("HAS_EXEC_NO_RELOC", I915_PARAM_HAS_EXEC_NO_RELOC),
        ("HAS_EXEC_HANDLE_LUT", I915_PARAM_HAS_EXEC_HANDLE_LUT),
        ("HAS_COHERENT_PHYS_GTT", I915_PARAM_HAS_COHERENT_PHYS_GTT),
        ("HAS_EXEC_SOFTPIN", I915_PARAM_HAS_EXEC_SOFTPIN),
        ("HAS_EXEC_ASYNC", I915_PARAM_HAS_EXEC_ASYNC),
        ("HAS_EXEC_FENCE", I915_PARAM_HAS_EXEC_FENCE),
        ("HAS_EXEC_CAPTURE", I915_PARAM_HAS_EXEC_CAPTURE),
        ("HAS_EXEC_BATCH_FIRST", I915_PARAM_HAS_EXEC_BATCH_FIRST),
        ("HAS_EXEC_FENCE_ARRAY", I915_PARAM_HAS_EXEC_FENCE_ARRAY),
        ("HAS_EXEC_SUBMIT_FENCE", I915_PARAM_HAS_EXEC_SUBMIT_FENCE),
        ("HAS_EXEC_TIMELINE_FENCES", I915_PARAM_HAS_EXEC_TIMELINE_FENCES),
        ("HAS_USERPTR_PROBE", I915_PARAM_HAS_USERPTR_PROBE),
    ] {
        assert_eq!(ok(&d, p), 1, "I915_PARAM_{name} (i915_getparam.c:132-153 -> value = 1)");
    }
}

#[test]
fn errors_linux_returns_for_this_device() {
    let d = gt2();
    // i915_getparam.c:105-113: HuC not loaded below Gen12 -> DISABLED -> -EOPNOTSUPP
    let e = err(&d, I915_PARAM_HUC_STATUS);
    assert_eq!((e.errno(), e.name()), (Errno::EOPNOTSUPP, "PARAM-REFUSED"));
    // :114-118: no PXP object -> -ENODEV (Mesa's i915_gem_supports_protected_context tests errno == ENODEV)
    assert_eq!(err(&d, I915_PARAM_PXP_STATUS).errno(), Errno::ENODEV);
    // :161-166: without GuC submission the ioctl SUCCEEDS with value -EINVAL
    assert_eq!(ok(&d, I915_PARAM_HAS_CONTEXT_FREQ_HINT), -22);
    // :28-33 old UMS/DRI1 parameters -> -ENODEV
    for p in [I915_PARAM_IRQ_ACTIVE, I915_PARAM_ALLOW_BATCHBUFFER, I915_PARAM_LAST_DISPATCH, I915_PARAM_HAS_EXEC_CONSTANTS] {
        assert_eq!(err(&d, p).errno(), Errno::ENODEV, "param {p}");
    }
    // :201-203 unknown -> -EINVAL: 0 is not a parameter; neither is one past the last; nor a "negative" int
    for p in [0, 60, 9999, 0x8000_0000, u32::MAX] {
        let e = err(&d, p);
        assert_eq!((e.errno(), e.name()), (Errno::EINVAL, "PARAM-UNKNOWN"), "param {p}");
    }
    // perf/OA is not ported: refused as unknown (what an older kernel says), by its own name
    for p in [I915_PARAM_PERF_REVISION, I915_PARAM_OA_TIMESTAMP_FREQUENCY] {
        let e = err(&d, p);
        assert_eq!((e.errno(), e.name()), (Errno::EINVAL, "PARAM-NOT-PORTED"));
    }
}

#[test]
fn every_param_mesa_issues_on_gen9_is_answered() {
    // The set iris and anv issue (docs/I915-UAPI-INVENTORY.md section 1), with the value or errno each must see.
    let d = gt2();
    let must_ok: [(u32, i32); 19] = [
        (I915_PARAM_CHIPSET_ID, 0x591c),
        (I915_PARAM_REVISION, 9),
        (I915_PARAM_CS_TIMESTAMP_FREQUENCY, 12_000_000),
        (I915_PARAM_SLICE_MASK, 1),
        (I915_PARAM_SUBSLICE_MASK, 7),
        (I915_PARAM_EU_TOTAL, 24),
        (I915_PARAM_MMAP_GTT_VERSION, 5),
        (I915_PARAM_HAS_USERPTR_PROBE, 1),
        (I915_PARAM_HAS_CONTEXT_ISOLATION, 15),
        (I915_PARAM_HAS_WAIT_TIMEOUT, 1),
        (I915_PARAM_HAS_EXECBUF2, 1),
        (I915_PARAM_MMAP_VERSION, 1),
        (I915_PARAM_HAS_EXEC_SOFTPIN, 1),
        (I915_PARAM_HAS_EXEC_FENCE_ARRAY, 1),
        (I915_PARAM_HAS_EXEC_ASYNC, 1),
        (I915_PARAM_HAS_EXEC_CAPTURE, 1),
        (I915_PARAM_HAS_EXEC_TIMELINE_FENCES, 1),
        (I915_PARAM_HAS_EXEC_NO_RELOC, 1),
        (I915_PARAM_HAS_EXEC_HANDLE_LUT, 1),
    ];
    for (p, v) in must_ok {
        assert_eq!(ok(&d, p), v, "param {p}");
    }
    // Mesa's intel_device_info.c:632 passes the BOOLEAN result of the previous getparam as the next selector: value 1 = IRQ_ACTIVE.
    assert_eq!(err(&d, 1).errno(), Errno::ENODEV);
}

#[test]
fn every_value_follows_the_fact_it_is_derived_from() {
    // Chipset id and engines come from the PlatformDescriptor: a GT3 row has the second VDBOX.
    let g3 = gt3();
    assert_eq!(ok(&g3, I915_PARAM_CHIPSET_ID), 0x5926);
    assert_eq!(ok(&g3, I915_PARAM_HAS_BSD2), 1);
    assert_eq!(ok(&g3, I915_PARAM_EU_TOTAL), 48);
    assert_eq!(ok(&g3, I915_PARAM_SLICE_MASK), 3);
    assert_eq!(ok(&g3, I915_PARAM_SUBSLICE_TOTAL), 6);

    // The timestamp frequency is the measured one, not a constant.
    let mut d = gt2();
    d.cs_timestamp_hz = 19_200_000;
    assert_eq!(ok(&d, I915_PARAM_CS_TIMESTAMP_FREQUENCY), 19_200_000);
    d.pci_revision = 0x0b;
    assert_eq!(ok(&d, I915_PARAM_REVISION), 11);

    // HAS_WT is HAS_EDRAM.
    d.edram_mb = 64;
    assert_eq!(ok(&d, I915_PARAM_HAS_WT), 1);

    // HAS_GPU_RESET: 0 / 1 / 2 as hangcheck, i915.reset and the row's has_reset_engine move (i915_getparam.c:90-95).
    let mut d = gt2();
    d.enable_hangcheck = false;
    assert_eq!(ok(&d, I915_PARAM_HAS_GPU_RESET), 0);
    d.enable_hangcheck = true;
    d.reset_param = 0;
    assert_eq!(ok(&d, I915_PARAM_HAS_GPU_RESET), 0);
    d.reset_param = 1;
    assert_eq!(ok(&d, I915_PARAM_HAS_GPU_RESET), 1);
    d.reset_param = 2;
    assert_eq!(ok(&d, I915_PARAM_HAS_GPU_RESET), 2);
    d.desc.info.has_reset_engine = false;
    assert_eq!(ok(&d, I915_PARAM_HAS_GPU_RESET), 1);

    // No mappable aperture -> no fence registers.
    let mut d = gt2();
    d.ggtt_has_aperture = false;
    assert_eq!(ok(&d, I915_PARAM_NUM_FENCES_AVAIL), 0);

    // Context isolation is the set of classes whose engines captured a default state.
    let mut d = gt2();
    d.default_state_classes = 0b0001;
    assert_eq!(ok(&d, I915_PARAM_HAS_CONTEXT_ISOLATION), 1);

    // The first-slice subslice mask and the totals come from the fuse decode: slice 0 with subslice 1 fused off.
    let d = kbl(0x591c, fuse2(0b001, 0b1010), [0, 0, 0]);
    assert_eq!(ok(&d, I915_PARAM_SUBSLICE_MASK), 0b0101);
    assert_eq!(ok(&d, I915_PARAM_SUBSLICE_TOTAL), 2);
    assert_eq!(ok(&d, I915_PARAM_EU_TOTAL), 16);

    // A part with no blitter has no BLT and no command parser.
    let mut d = gt2();
    d.desc.info.engine_mask &= !(1 << i915_devinfo_core::engine::BCS0);
    assert_eq!(ok(&d, I915_PARAM_HAS_BLT), 0);
    assert_eq!(ok(&d, I915_PARAM_CMD_PARSER_VERSION), 0);
    assert_eq!(ok(&d, I915_PARAM_HAS_CONTEXT_ISOLATION) & 0b0010, 0b0010, "default_state_classes is a separate fact, captured by the GT");

    // No LLC (hypothetical row) flips HAS_LLC; no HuC support flips HUC_STATUS from EOPNOTSUPP to ENODEV.
    let mut d = gt2();
    d.desc.info.has_llc = false;
    assert_eq!(ok(&d, I915_PARAM_HAS_LLC), 0);
    d.desc.info.has_gt_uc = false;
    assert_eq!(err(&d, I915_PARAM_HUC_STATUS).errno(), Errno::ENODEV);
}

#[test]
fn zero_sseu_answers_enodev_as_linux_does() {
    // i915_getparam.c:80-84,85-89,170-178,179-188
    let mut d = gt2();
    d.sseu = gen9_sseu_info(0, [0, 0, 0]); // no slice enabled
    for p in [I915_PARAM_SUBSLICE_TOTAL, I915_PARAM_EU_TOTAL, I915_PARAM_SLICE_MASK, I915_PARAM_SUBSLICE_MASK] {
        assert_eq!(err(&d, p).errno(), Errno::ENODEV, "param {p}");
    }
}

#[test]
fn cherryview_is_refused_not_aliased() {
    let desc = i915_devinfo_core::lookup(0x22b0).unwrap();
    let r = UapiDevice::new(desc, 0, gen9_sseu_info(0, [0; 3]), 0, 0);
    assert_eq!(r.unwrap_err().name(), "UNSUPPORTED-GENERATION");
}

#[test]
fn wire_request_and_reply_bytes() {
    let d = gt2();
    let mut out = [0u8; 64];
    // request: param=4 (CHIPSET_ID), reserved=0
    let n = handle_getparam(&d, &[4, 0, 0, 0, 0, 0, 0, 0], &mut out).unwrap();
    assert_eq!(n, 16);
    // status Ok(0), errno 0, value 0x591c, pad
    assert_eq!(&out[..16], &[0, 0, 0, 0, 0, 0, 0, 0, 0x1c, 0x59, 0, 0, 0, 0, 0, 0]);
    // a refused param is a well-formed reply carrying -errno: PXP_STATUS = 58 -> -ENODEV (-19)
    let n = handle_getparam(&d, &[58, 0, 0, 0, 0, 0, 0, 0], &mut out).unwrap();
    assert_eq!(n, 16);
    assert_eq!(&out[..16], &[0, 0, 0, 0, 0xed, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0]);
    // malformed requests are refused by name
    assert_eq!(decode_getparam_request(&[4, 0, 0, 0]).unwrap_err().name(), "WIRE-TRUNCATED");
    assert_eq!(decode_getparam_request(&[4, 0, 0, 0, 1, 0, 0, 0]).unwrap_err().name(), "WIRE-RESERVED-NONZERO");
    assert_eq!(decode_getparam_request(&[4, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap_err().name(), "WIRE-TRAILING-BYTES");
    // a reply buffer that is too small is refused, not truncated
    let mut small = [0u8; 8];
    assert!(encode_getparam_reply(&Ok(1), &mut small).is_err());
}
