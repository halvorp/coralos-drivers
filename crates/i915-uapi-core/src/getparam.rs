// SPDX-License-Identifier: GPL-2.0-only
//! `DRM_IOCTL_I915_GETPARAM`: a port of `i915_getparam_ioctl` (drivers/gpu/drm/i915/i915_getparam.c:17-210, v7.0-rc6) for a Gen9
//! device. Every value is derived from the [`UapiDevice`] (PlatformDescriptor + measured facts); the only literals are the ones
//! Linux itself hard-codes (the always-1 feature bits, the mmap versions, the cmd-parser version).
//!
//! Wire: request payload = `param: u32`, `reserved: u32 (zero)`; reply payload = see [`encode_getparam_reply`].
use crate::codec::{Reader, Writer};
use crate::consts::param as p;
use crate::device::UapiDevice;
use crate::errno::Errno;
use crate::error::UapiError;
use crate::consts::misc as m;
use crate::types::engine_class;

pub const GETPARAM_REQUEST_BYTES: usize = 8;
pub const GETPARAM_REPLY_BYTES: usize = 16;

/// `i915_gem_mmap_gtt_version()` (gem/i915_gem_mman.c:194-197).
pub const MMAP_GTT_VERSION: i32 = 5; // i915_gem_mman.c:196
/// `I915_PARAM_MMAP_VERSION` is a constant 1 ("remember to bump this if the version changes", i915_getparam.c:130-131,159).
pub const MMAP_VERSION: i32 = 1; // i915_getparam.c:159
/// `i915_cmd_parser_get_version()` once a parser is active (i915_cmd_parser.c:1621-1623).
pub const CMD_PARSER_VERSION: i32 = 10; // i915_cmd_parser.c:1623
/// `GEN7_FEATURES`-style fence count on Gen7+ that is not VLV/CHV (gt/intel_ggtt_fencing.c:855-857).
pub const GEN7_PLUS_FENCES: i32 = 32; // intel_ggtt_fencing.c:857

/// The answer to one GETPARAM: the value `put_user` would store, or the refusal with the errno Linux returns.
pub fn getparam(dev: &UapiDevice, param: u32) -> Result<i32, UapiError> {
    // `drm_i915_getparam::param` is a signed int in C (i915_drm.h:820-823): a negative number matches no case below.
    match param {
        p::I915_PARAM_IRQ_ACTIVE | p::I915_PARAM_ALLOW_BATCHBUFFER | p::I915_PARAM_LAST_DISPATCH | p::I915_PARAM_HAS_EXEC_CONSTANTS => {
            Err(UapiError::ParamLegacyUms { param }) // i915_getparam.c:28-33
        }
        p::I915_PARAM_CHIPSET_ID => Ok(i32::from(dev.desc.devid)), // :34-36 (pdev->device)
        p::I915_PARAM_REVISION => Ok(i32::from(dev.pci_revision)), // :37-39 (pdev->revision)
        p::I915_PARAM_NUM_FENCES_AVAIL => Ok(num_fences(dev)),     // :40-42
        p::I915_PARAM_HAS_OVERLAY => Ok(0), // :43-45 intel_overlay_available(): only the gen2-4 overlay engine; Gen9 has none
        p::I915_PARAM_HAS_BSD => Ok(has_engine(dev, engine_class::VIDEO, 0)),  // :46-49
        p::I915_PARAM_HAS_BLT => Ok(has_engine(dev, engine_class::COPY, 0)),   // :50-53
        p::I915_PARAM_HAS_VEBOX => Ok(has_engine(dev, engine_class::VIDEO_ENHANCE, 0)), // :54-57
        p::I915_PARAM_HAS_BSD2 => Ok(has_engine(dev, engine_class::VIDEO, 1)), // :58-61
        p::I915_PARAM_HAS_LLC => Ok(i32::from(dev.desc.info.has_llc)), // :62-64 HAS_LLC
        p::I915_PARAM_HAS_WT => Ok(i32::from(dev.edram_mb != 0)),      // :65-67 HAS_WT == HAS_EDRAM (i915_drv.h:557)
        p::I915_PARAM_HAS_ALIASING_PPGTT => Ok(dev.ppgtt_type() as i32), // :68-70 INTEL_PPGTT
        p::I915_PARAM_HAS_SEMAPHORES => Ok(i32::from(dev.scheduler_caps() & m::I915_SCHEDULER_CAP_SEMAPHORES != 0)), // :71-73
        p::I915_PARAM_HAS_SECURE_BATCHES => Ok(0), // :74-76 HAS_SECURE_BATCHES is GRAPHICS_VER < 6 (i915_drv.h:556)
        p::I915_PARAM_CMD_PARSER_VERSION => Ok(cmd_parser_version(dev)), // :77-79
        p::I915_PARAM_SUBSLICE_TOTAL => nonzero_or_enodev(dev.sseu.subslice_total() as i32, param, "no subslices in the SSEU"), // :80-84
        p::I915_PARAM_EU_TOTAL => nonzero_or_enodev(i32::from(dev.sseu.eu_total), param, "no EUs in the SSEU"), // :85-89
        p::I915_PARAM_HAS_GPU_RESET => Ok(has_gpu_reset(dev)), // :90-95
        p::I915_PARAM_HAS_RESOURCE_STREAMER => Ok(0), // :96-98
        p::I915_PARAM_HAS_POOLED_EU => Ok(0), // :99-101 HAS_POOLED_EU is set for Gen9 LP only (gt/intel_sseu.c gen9_sseu_info_init)
        p::I915_PARAM_MIN_EU_IN_POOL => Ok(0), // :102-104 sseu->min_eu_in_pool is Gen9 LP only
        p::I915_PARAM_HUC_STATUS => {
            // :105-113 intel_huc_check_status(): with `i915.enable_guc` defaulting to 0 below Gen12 (intel_uc.c:34-35) the HuC
            // firmware is DISABLED (__override_huc_firmware_path -> "", intel_uc_fw.c:455-460) -> -EOPNOTSUPP; a row without
            // `has_gt_uc` never selects a blob -> NOT_SUPPORTED -> -ENODEV (intel_uc_fw.c:528-542, intel_huc.c:611-614).
            if dev.desc.info.has_gt_uc {
                Err(UapiError::ParamRefused { param, errno: Errno::EOPNOTSUPP, why: "HuC firmware disabled (enable_guc=0 below Gen12)" })
            } else {
                Err(UapiError::ParamRefused { param, errno: Errno::ENODEV, why: "no GT microcontrollers" })
            }
        }
        p::I915_PARAM_PXP_STATUS => {
            Err(UapiError::ParamRefused { param, errno: Errno::ENODEV, why: "no PXP on this platform" }) // :114-118, intel_pxp.c:398-401
        }
        p::I915_PARAM_MMAP_GTT_VERSION => Ok(MMAP_GTT_VERSION), // :119-125
        p::I915_PARAM_HAS_SCHEDULER => Ok(dev.scheduler_caps() as i32), // :126-128
        p::I915_PARAM_MMAP_VERSION => Ok(MMAP_VERSION), // :130-131
        // :132-160 "For the time being all of these are always true".
        p::I915_PARAM_HAS_GEM
        | p::I915_PARAM_HAS_PAGEFLIPPING
        | p::I915_PARAM_HAS_EXECBUF2
        | p::I915_PARAM_HAS_RELAXED_FENCING
        | p::I915_PARAM_HAS_COHERENT_RINGS
        | p::I915_PARAM_HAS_RELAXED_DELTA
        | p::I915_PARAM_HAS_GEN7_SOL_RESET
        | p::I915_PARAM_HAS_WAIT_TIMEOUT
        | p::I915_PARAM_HAS_PRIME_VMAP_FLUSH
        | p::I915_PARAM_HAS_PINNED_BATCHES
        | p::I915_PARAM_HAS_EXEC_NO_RELOC
        | p::I915_PARAM_HAS_EXEC_HANDLE_LUT
        | p::I915_PARAM_HAS_COHERENT_PHYS_GTT
        | p::I915_PARAM_HAS_EXEC_SOFTPIN
        | p::I915_PARAM_HAS_EXEC_ASYNC
        | p::I915_PARAM_HAS_EXEC_FENCE
        | p::I915_PARAM_HAS_EXEC_CAPTURE
        | p::I915_PARAM_HAS_EXEC_BATCH_FIRST
        | p::I915_PARAM_HAS_EXEC_FENCE_ARRAY
        | p::I915_PARAM_HAS_EXEC_SUBMIT_FENCE
        | p::I915_PARAM_HAS_EXEC_TIMELINE_FENCES
        | p::I915_PARAM_HAS_USERPTR_PROBE => Ok(1),
        // :161-166 `value = -EINVAL` WITHOUT returning: Linux answers success with the value -22 when GuC submission is off.
        p::I915_PARAM_HAS_CONTEXT_FREQ_HINT => Ok(Errno::EINVAL.neg()),
        p::I915_PARAM_HAS_CONTEXT_ISOLATION => Ok(dev.default_state_classes as i32), // :167-169
        p::I915_PARAM_SLICE_MASK => {
            // :170-178 "Not supported from Xe_HP onward": GRAPHICS_VER_FULL >= 12.55. Gen9 is below; kept as the guard.
            let v = i32::from(dev.sseu.slice_mask);
            nonzero_or_enodev(v, param, "slice mask is zero")
        }
        p::I915_PARAM_SUBSLICE_MASK => {
            // :179-188 "Only copy bits from the first slice": intel_sseu_get_hsw_subslices(sseu, 0).
            let v = i32::from(dev.sseu.subslice_mask[0]);
            nonzero_or_enodev(v, param, "first slice has no subslices")
        }
        p::I915_PARAM_CS_TIMESTAMP_FREQUENCY => Ok(dev.cs_timestamp_hz as i32), // :189-191
        p::I915_PARAM_MMAP_GTT_COHERENT => Ok(i32::from(dev.desc.info.has_coherent_ggtt)), // :192-194
        p::I915_PARAM_PERF_REVISION | p::I915_PARAM_OA_TIMESTAMP_FREQUENCY => {
            // :195-200 i915 perf / OA is not ported (KBL-I915-PORT-PLAN section 1: "perf/OA until something needs it").
            Err(UapiError::ParamNotPorted { param, why: "i915 perf/OA is not ported" })
        }
        _ => Err(UapiError::ParamUnknown { param }), // :201-203
    }
}

fn nonzero_or_enodev(v: i32, param: u32, why: &'static str) -> Result<i32, UapiError> {
    if v == 0 {
        Err(UapiError::ParamRefused { param, errno: Errno::ENODEV, why })
    } else {
        Ok(v)
    }
}

fn has_engine(dev: &UapiDevice, class: u16, instance: u16) -> i32 {
    i32::from(dev.engines().lookup(class, instance).is_some())
}

/// `ggtt->num_fences` (gt/intel_ggtt_fencing.c:845-863) for the carried rows: 32 on Gen7+ (none is VLV/CHV once `require_gen9`
/// passed) when the GGTT has a mappable aperture, else 0.
fn num_fences(dev: &UapiDevice) -> i32 {
    if dev.ggtt_has_aperture && dev.gen() >= 7 {
        GEN7_PLUS_FENCES
    } else {
        0
    }
}

/// `i915_cmd_parser_get_version()` (i915_cmd_parser.c:1589-1624): zero unless some user engine runs the command parser. Gen9
/// parses the BLITTER only (`intel_engine_init_cmd_parser`, i915_cmd_parser.c:961-963 gates on Gen7 or Gen9-COPY).
fn cmd_parser_version(dev: &UapiDevice) -> i32 {
    let active = dev.engines().as_slice().iter().any(|e| match dev.gen() {
        7 => true,
        9 => e.class == engine_class::COPY,
        _ => false,
    });
    if active {
        CMD_PARSER_VERSION
    } else {
        0
    }
}

/// `enable_hangcheck && intel_has_gpu_reset()`, upgraded to 2 when engine reset exists (i915_getparam.c:90-95;
/// intel_reset.c:795-809): `has_gpu_reset` needs `i915.reset != 0` (every Gen6+ part has a full reset), engine reset needs
/// `i915.reset >= 2` and the row's `has_reset_engine`.
fn has_gpu_reset(dev: &UapiDevice) -> i32 {
    let full = dev.enable_hangcheck && dev.reset_param != 0;
    if !full {
        return 0;
    }
    if dev.reset_param >= 2 && dev.desc.info.has_reset_engine {
        2
    } else {
        1
    }
}

/// Decode a GETPARAM request payload: `param`, then a reserved word that must be zero.
pub fn decode_getparam_request(payload: &[u8]) -> Result<u32, UapiError> {
    let mut r = Reader::new(payload);
    let param = r.u32("getparam.param")?;
    let rsvd = r.u32("getparam.reserved")?;
    if rsvd != 0 {
        return Err(UapiError::ReservedNonZero { what: "getparam.reserved", got: u64::from(rsvd) });
    }
    if r.remaining() != 0 {
        return Err(UapiError::TrailingBytes { extra: r.remaining() });
    }
    Ok(param)
}

/// Encode the reply payload (16 bytes): `status: u32` ([`i915_wire::Status`]), `errno: i32` (0, or `-errno`), `value: i32`, `pad: u32`.
/// A refused GETPARAM is still a well-formed reply (`Status::Ok`) carrying the negative errno, as the ioctl returns it.
pub fn encode_getparam_reply(result: &Result<i32, UapiError>, out: &mut [u8]) -> Result<usize, UapiError> {
    let mut w = Writer::new(out);
    w.u32(i915_wire::Status::Ok as u32)?;
    match result {
        Ok(v) => {
            w.u32(0)?;
            w.u32(*v as u32)?;
        }
        Err(e) => {
            w.u32(e.errno().neg() as u32)?;
            w.u32(0)?;
        }
    }
    w.u32(0)?;
    Ok(w.pos())
}

/// Decode the request and answer it in one step: the device entry point for `Opcode`-routed GETPARAM frames. A malformed payload
/// returns its error (the device answers `Status::Malformed`); a well-formed request always yields an encoded reply.
pub fn handle_getparam(dev: &UapiDevice, payload: &[u8], out: &mut [u8]) -> Result<usize, UapiError> {
    let param = decode_getparam_request(payload)?;
    encode_getparam_reply(&getparam(dev, param), out)
}
