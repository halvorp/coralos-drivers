// SPDX-License-Identifier: GPL-2.0-only
//! The uAPI constants, pinned BY HAND against include/uapi/drm/i915_drm.h (v7.0-rc6). The generator derived the production
//! values from the same header; these literals were typed from the header listing, so a generator bug cannot pass.
use i915_uapi_core::consts::*;
use i915_uapi_core::execbuf::{EXEC_FENCE_UNKNOWN_FLAGS, EXEC_OBJECT_UNKNOWN_FLAGS, GPU_DOMAINS};
use i915_uapi_core::execbuf_wire::{ILLEGAL_FLAGS, UNKNOWN_FLAGS};

#[test]
fn all_59_getparam_selectors_by_value_and_name() {
    use param::*;
    // i915_drm.h:597-813, one line per #define, in header order.
    let table: [(&str, u32, u32); 59] = [
        ("IRQ_ACTIVE", I915_PARAM_IRQ_ACTIVE, 1),
        ("ALLOW_BATCHBUFFER", I915_PARAM_ALLOW_BATCHBUFFER, 2),
        ("LAST_DISPATCH", I915_PARAM_LAST_DISPATCH, 3),
        ("CHIPSET_ID", I915_PARAM_CHIPSET_ID, 4),
        ("HAS_GEM", I915_PARAM_HAS_GEM, 5),
        ("NUM_FENCES_AVAIL", I915_PARAM_NUM_FENCES_AVAIL, 6),
        ("HAS_OVERLAY", I915_PARAM_HAS_OVERLAY, 7),
        ("HAS_PAGEFLIPPING", I915_PARAM_HAS_PAGEFLIPPING, 8),
        ("HAS_EXECBUF2", I915_PARAM_HAS_EXECBUF2, 9),
        ("HAS_BSD", I915_PARAM_HAS_BSD, 10),
        ("HAS_BLT", I915_PARAM_HAS_BLT, 11),
        ("HAS_RELAXED_FENCING", I915_PARAM_HAS_RELAXED_FENCING, 12),
        ("HAS_COHERENT_RINGS", I915_PARAM_HAS_COHERENT_RINGS, 13),
        ("HAS_EXEC_CONSTANTS", I915_PARAM_HAS_EXEC_CONSTANTS, 14),
        ("HAS_RELAXED_DELTA", I915_PARAM_HAS_RELAXED_DELTA, 15),
        ("HAS_GEN7_SOL_RESET", I915_PARAM_HAS_GEN7_SOL_RESET, 16),
        ("HAS_LLC", I915_PARAM_HAS_LLC, 17),
        ("HAS_ALIASING_PPGTT", I915_PARAM_HAS_ALIASING_PPGTT, 18),
        ("HAS_WAIT_TIMEOUT", I915_PARAM_HAS_WAIT_TIMEOUT, 19),
        ("HAS_SEMAPHORES", I915_PARAM_HAS_SEMAPHORES, 20),
        ("HAS_PRIME_VMAP_FLUSH", I915_PARAM_HAS_PRIME_VMAP_FLUSH, 21),
        ("HAS_VEBOX", I915_PARAM_HAS_VEBOX, 22),
        ("HAS_SECURE_BATCHES", I915_PARAM_HAS_SECURE_BATCHES, 23),
        ("HAS_PINNED_BATCHES", I915_PARAM_HAS_PINNED_BATCHES, 24),
        ("HAS_EXEC_NO_RELOC", I915_PARAM_HAS_EXEC_NO_RELOC, 25),
        ("HAS_EXEC_HANDLE_LUT", I915_PARAM_HAS_EXEC_HANDLE_LUT, 26),
        ("HAS_WT", I915_PARAM_HAS_WT, 27),
        ("CMD_PARSER_VERSION", I915_PARAM_CMD_PARSER_VERSION, 28),
        ("HAS_COHERENT_PHYS_GTT", I915_PARAM_HAS_COHERENT_PHYS_GTT, 29),
        ("MMAP_VERSION", I915_PARAM_MMAP_VERSION, 30),
        ("HAS_BSD2", I915_PARAM_HAS_BSD2, 31),
        ("REVISION", I915_PARAM_REVISION, 32),
        ("SUBSLICE_TOTAL", I915_PARAM_SUBSLICE_TOTAL, 33),
        ("EU_TOTAL", I915_PARAM_EU_TOTAL, 34),
        ("HAS_GPU_RESET", I915_PARAM_HAS_GPU_RESET, 35),
        ("HAS_RESOURCE_STREAMER", I915_PARAM_HAS_RESOURCE_STREAMER, 36),
        ("HAS_EXEC_SOFTPIN", I915_PARAM_HAS_EXEC_SOFTPIN, 37),
        ("HAS_POOLED_EU", I915_PARAM_HAS_POOLED_EU, 38),
        ("MIN_EU_IN_POOL", I915_PARAM_MIN_EU_IN_POOL, 39),
        ("MMAP_GTT_VERSION", I915_PARAM_MMAP_GTT_VERSION, 40),
        ("HAS_SCHEDULER", I915_PARAM_HAS_SCHEDULER, 41),
        ("HUC_STATUS", I915_PARAM_HUC_STATUS, 42),
        ("HAS_EXEC_ASYNC", I915_PARAM_HAS_EXEC_ASYNC, 43),
        ("HAS_EXEC_FENCE", I915_PARAM_HAS_EXEC_FENCE, 44),
        ("HAS_EXEC_CAPTURE", I915_PARAM_HAS_EXEC_CAPTURE, 45),
        ("SLICE_MASK", I915_PARAM_SLICE_MASK, 46),
        ("SUBSLICE_MASK", I915_PARAM_SUBSLICE_MASK, 47),
        ("HAS_EXEC_BATCH_FIRST", I915_PARAM_HAS_EXEC_BATCH_FIRST, 48),
        ("HAS_EXEC_FENCE_ARRAY", I915_PARAM_HAS_EXEC_FENCE_ARRAY, 49),
        ("HAS_CONTEXT_ISOLATION", I915_PARAM_HAS_CONTEXT_ISOLATION, 50),
        ("CS_TIMESTAMP_FREQUENCY", I915_PARAM_CS_TIMESTAMP_FREQUENCY, 51),
        ("MMAP_GTT_COHERENT", I915_PARAM_MMAP_GTT_COHERENT, 52),
        ("HAS_EXEC_SUBMIT_FENCE", I915_PARAM_HAS_EXEC_SUBMIT_FENCE, 53),
        ("PERF_REVISION", I915_PARAM_PERF_REVISION, 54),
        ("HAS_EXEC_TIMELINE_FENCES", I915_PARAM_HAS_EXEC_TIMELINE_FENCES, 55),
        ("HAS_USERPTR_PROBE", I915_PARAM_HAS_USERPTR_PROBE, 56),
        ("OA_TIMESTAMP_FREQUENCY", I915_PARAM_OA_TIMESTAMP_FREQUENCY, 57),
        ("PXP_STATUS", I915_PARAM_PXP_STATUS, 58),
        ("HAS_CONTEXT_FREQ_HINT", I915_PARAM_HAS_CONTEXT_FREQ_HINT, 59),
    ];
    for (name, got, want) in table {
        assert_eq!(got, want, "I915_PARAM_{name}");
    }
    // The 59 selectors are exactly 1..=59, each once: no gap, no duplicate.
    let mut seen = [false; 60];
    for (_, got, _) in table {
        assert!(!seen[got as usize], "duplicate selector {got}");
        seen[got as usize] = true;
    }
    assert!(seen[1..].iter().all(|s| *s));
}

#[test]
fn execbuffer2_flags_by_value() {
    use exec::*;
    assert_eq!(I915_EXEC_RING_MASK, 0x3f); // :1431
    assert_eq!(I915_EXEC_DEFAULT, 0);
    assert_eq!(I915_EXEC_RENDER, 1);
    assert_eq!(I915_EXEC_BSD, 2);
    assert_eq!(I915_EXEC_BLT, 3);
    assert_eq!(I915_EXEC_VEBOX, 4);
    assert_eq!(I915_EXEC_CONSTANTS_MASK, 3 << 6); // :1444
    assert_eq!(I915_EXEC_GEN7_SOL_RESET, 1 << 8);
    assert_eq!(I915_EXEC_SECURE, 1 << 9);
    assert_eq!(I915_EXEC_IS_PINNED, 1 << 10);
    assert_eq!(I915_EXEC_NO_RELOC, 0x800);
    assert_eq!(I915_EXEC_HANDLE_LUT, 0x1000);
    assert_eq!(I915_EXEC_BSD_SHIFT, 13);
    assert_eq!(I915_EXEC_BSD_MASK, 0x6000);
    assert_eq!(I915_EXEC_BSD_DEFAULT, 0);
    assert_eq!(I915_EXEC_BSD_RING1, 0x2000);
    assert_eq!(I915_EXEC_BSD_RING2, 0x4000);
    assert_eq!(I915_EXEC_RESOURCE_STREAMER, 0x8000);
    assert_eq!(I915_EXEC_FENCE_IN, 0x1_0000);
    assert_eq!(I915_EXEC_FENCE_OUT, 0x2_0000);
    assert_eq!(I915_EXEC_BATCH_FIRST, 0x4_0000);
    assert_eq!(I915_EXEC_FENCE_ARRAY, 0x8_0000);
    assert_eq!(I915_EXEC_FENCE_SUBMIT, 0x10_0000);
    assert_eq!(I915_EXEC_USE_EXTENSIONS, 0x20_0000);
    // __I915_EXEC_UNKNOWN_FLAGS = -(USE_EXTENSIONS << 1) = -(1 << 22); ILLEGAL adds CONSTANTS_MASK and RESOURCE_STREAMER (:73-76)
    assert_eq!(UNKNOWN_FLAGS, 0xffff_ffff_ffc0_0000);
    assert_eq!(ILLEGAL_FLAGS, 0xffff_ffff_ffc0_0000 | 0xc0 | 0x8000);
}

#[test]
fn exec_object_and_fence_flags_by_value() {
    use exec_object::*;
    assert_eq!(EXEC_OBJECT_NEEDS_FENCE, 1);
    assert_eq!(EXEC_OBJECT_NEEDS_GTT, 2);
    assert_eq!(EXEC_OBJECT_WRITE, 4);
    assert_eq!(EXEC_OBJECT_SUPPORTS_48B_ADDRESS, 8);
    assert_eq!(EXEC_OBJECT_PINNED, 0x10);
    assert_eq!(EXEC_OBJECT_PAD_TO_SIZE, 0x20);
    assert_eq!(EXEC_OBJECT_ASYNC, 0x40);
    assert_eq!(EXEC_OBJECT_CAPTURE, 0x80);
    assert_eq!(EXEC_OBJECT_UNKNOWN_FLAGS, 0xffff_ffff_ffff_ff00); // -(CAPTURE << 1)
    assert_eq!(exec_fence::I915_EXEC_FENCE_WAIT, 1);
    assert_eq!(exec_fence::I915_EXEC_FENCE_SIGNAL, 2);
    assert_eq!(EXEC_FENCE_UNKNOWN_FLAGS, 0xffff_fffc); // -(SIGNAL << 1)
    assert_eq!(exec_fence::DRM_I915_GEM_EXECBUFFER_EXT_TIMELINE_FENCES, 0);
}

#[test]
fn query_ids_command_numbers_domains_and_context_params() {
    use query::*;
    assert_eq!(DRM_I915_QUERY_TOPOLOGY_INFO, 1);
    assert_eq!(DRM_I915_QUERY_ENGINE_INFO, 2);
    assert_eq!(DRM_I915_QUERY_PERF_CONFIG, 3);
    assert_eq!(DRM_I915_QUERY_MEMORY_REGIONS, 4);
    assert_eq!(DRM_I915_QUERY_HWCONFIG_BLOB, 5);
    assert_eq!(DRM_I915_QUERY_GEOMETRY_SUBSLICES, 6);
    assert_eq!(DRM_I915_QUERY_GUC_SUBMISSION_VERSION, 7);
    assert_eq!(DRM_I915_QUERY_PERF_CONFIG_LIST, 1);
    assert_eq!(DRM_I915_QUERY_PERF_CONFIG_DATA_FOR_UUID, 2);
    assert_eq!(DRM_I915_QUERY_PERF_CONFIG_DATA_FOR_ID, 3);

    use cmd::*;
    assert_eq!(DRM_I915_GETPARAM, 0x06);
    assert_eq!(DRM_I915_SETPARAM, 0x07);
    assert_eq!(DRM_I915_GEM_BUSY, 0x17);
    assert_eq!(DRM_I915_GEM_CREATE, 0x1b);
    assert_eq!(DRM_I915_GEM_MMAP, 0x1e);
    assert_eq!(DRM_I915_GEM_SET_DOMAIN, 0x1f);
    assert_eq!(DRM_I915_GEM_SET_TILING, 0x21);
    assert_eq!(DRM_I915_GEM_GET_TILING, 0x22);
    assert_eq!(DRM_I915_GEM_GET_APERTURE, 0x23);
    assert_eq!(DRM_I915_GEM_MMAP_GTT, 0x24);
    assert_eq!(DRM_I915_GEM_MADVISE, 0x26);
    assert_eq!(DRM_I915_GEM_EXECBUFFER2, 0x29);
    assert_eq!(DRM_I915_GEM_EXECBUFFER2_WR, 0x29);
    assert_eq!(DRM_I915_GEM_WAIT, 0x2c);
    assert_eq!(DRM_I915_GEM_CONTEXT_CREATE, 0x2d);
    assert_eq!(DRM_I915_GEM_CONTEXT_DESTROY, 0x2e);
    assert_eq!(DRM_I915_GEM_SET_CACHING, 0x2f);
    assert_eq!(DRM_I915_REG_READ, 0x31);
    assert_eq!(DRM_I915_GET_RESET_STATS, 0x32);
    assert_eq!(DRM_I915_GEM_USERPTR, 0x33);
    assert_eq!(DRM_I915_GEM_CONTEXT_GETPARAM, 0x34);
    assert_eq!(DRM_I915_GEM_CONTEXT_SETPARAM, 0x35);
    assert_eq!(DRM_I915_QUERY, 0x39);
    assert_eq!(DRM_I915_GEM_VM_CREATE, 0x3a);
    assert_eq!(DRM_I915_GEM_VM_DESTROY, 0x3b);
    assert_eq!(DRM_I915_GEM_CREATE_EXT, 0x3c);

    use domain::*;
    assert_eq!(I915_GEM_DOMAIN_CPU, 1);
    assert_eq!(I915_GEM_DOMAIN_RENDER, 2);
    assert_eq!(I915_GEM_DOMAIN_SAMPLER, 4);
    assert_eq!(I915_GEM_DOMAIN_COMMAND, 8);
    assert_eq!(I915_GEM_DOMAIN_INSTRUCTION, 0x10);
    assert_eq!(I915_GEM_DOMAIN_VERTEX, 0x20);
    assert_eq!(I915_GEM_DOMAIN_GTT, 0x40);
    assert_eq!(I915_GEM_DOMAIN_WC, 0x80);
    // I915_GEM_GPU_DOMAINS (i915_gem.h:42-47) = RENDER|SAMPLER|COMMAND|INSTRUCTION|VERTEX: neither CPU, GTT nor WC.
    assert_eq!(GPU_DOMAINS, 0x2 | 0x4 | 0x8 | 0x10 | 0x20);

    use ctx_param::*;
    assert_eq!(I915_CONTEXT_PARAM_BAN_PERIOD, 1);
    assert_eq!(I915_CONTEXT_PARAM_GTT_SIZE, 3);
    assert_eq!(I915_CONTEXT_PARAM_NO_ERROR_CAPTURE, 4);
    assert_eq!(I915_CONTEXT_PARAM_BANNABLE, 5);
    assert_eq!(I915_CONTEXT_PARAM_PRIORITY, 6);
    assert_eq!(I915_CONTEXT_PARAM_SSEU, 7);
    assert_eq!(I915_CONTEXT_PARAM_RECOVERABLE, 8);
    assert_eq!(I915_CONTEXT_PARAM_VM, 9);
    assert_eq!(I915_CONTEXT_PARAM_ENGINES, 0xa);
    assert_eq!(I915_CONTEXT_PARAM_PERSISTENCE, 0xb);
    assert_eq!(I915_CONTEXT_PARAM_RINGSIZE, 0xc);
    assert_eq!(I915_CONTEXT_PARAM_PROTECTED_CONTENT, 0xd);
    assert_eq!(I915_CONTEXT_PARAM_LOW_LATENCY, 0xe);
    assert_eq!(I915_CONTEXT_PARAM_CONTEXT_IMAGE, 0xf);
}

#[test]
fn misc_modes_by_value() {
    use misc::*;
    assert_eq!((I915_MMAP_OFFSET_GTT, I915_MMAP_OFFSET_WC, I915_MMAP_OFFSET_WB, I915_MMAP_OFFSET_UC, I915_MMAP_OFFSET_FIXED), (0, 1, 2, 3, 4));
    assert_eq!((I915_TILING_NONE, I915_TILING_X, I915_TILING_Y, I915_TILING_LAST), (0, 1, 2, 2));
    assert_eq!((I915_MADV_WILLNEED, I915_MADV_DONTNEED), (0, 1));
    assert_eq!((I915_CACHING_NONE, I915_CACHING_CACHED, I915_CACHING_DISPLAY), (0, 1, 2));
    assert_eq!((I915_GEM_CREATE_EXT_MEMORY_REGIONS, I915_GEM_CREATE_EXT_PROTECTED_CONTENT, I915_GEM_CREATE_EXT_SET_PAT), (0, 1, 2));
    assert_eq!(I915_GEM_CREATE_EXT_FLAG_NEEDS_CPU_ACCESS, 1);
    assert_eq!((I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS, I915_CONTEXT_CREATE_FLAGS_SINGLE_TIMELINE), (1, 2));
    assert_eq!((I915_CONTEXT_CREATE_EXT_SETPARAM, I915_CONTEXT_CREATE_EXT_CLONE), (0, 1));
    assert_eq!((I915_USERPTR_READ_ONLY, I915_USERPTR_PROBE, I915_USERPTR_UNSYNCHRONIZED), (1, 2, 0x8000_0000));
    assert_eq!(I915_ENGINE_INFO_HAS_LOGICAL_INSTANCE, 1);
    assert_eq!((I915_VIDEO_CLASS_CAPABILITY_HEVC, I915_VIDEO_AND_ENHANCE_CLASS_CAPABILITY_SFC), (1, 2));
    assert_eq!(
        (I915_SCHEDULER_CAP_ENABLED, I915_SCHEDULER_CAP_PRIORITY, I915_SCHEDULER_CAP_PREEMPTION, I915_SCHEDULER_CAP_SEMAPHORES, I915_SCHEDULER_CAP_ENGINE_BUSY_STATS),
        (1, 2, 4, 8, 16)
    );
    assert_eq!(I915_SCHEDULER_CAP_STATIC_PRIORITY_MAP, 32);
    assert_eq!((I915_CONTEXT_MAX_USER_PRIORITY, I915_CONTEXT_DEFAULT_PRIORITY, I915_CONTEXT_MIN_USER_PRIORITY), (1023, 0, -1023));
    assert_eq!(I915_MMAP_WC, 1);
}
