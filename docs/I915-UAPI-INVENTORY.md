# i915 uAPI call inventory: Mesa iris + anv on a Gen9 device (Kaby Lake, 8086:591C)

Wave 1 deliverable of `KBL-I915-PORT-PLAN.md` (section 2: "freeze from the pinned Mesa by tracing the real call inventory before
claiming a minimum"). Every ioctl, GETPARAM, I915_QUERY item, context param, execbuffer2 flag, syncobj / timeline / PRIME call that
iris and anv issue on a Gen9 device, with the Mesa `file:line` of the call site and its status.

## Provenance

| Tree | Path | Revision |
|---|---|---|
| Mesa (CoralOS-patched fork) | `~/Projects/references/coralos/mesa` | `b8fa4fd2581166e5fcbaef7471dad11556572d4f` |
| libdrm | `~/Projects/references/coralos/libdrm` | as checked out (`build-coral/` present) |
| Linux (the uAPI the port ports) | `~/Projects/references/coralos/linux-ref` | v7.0-rc6 |

Mesa paths below are relative to `src/`. `common/` is `intel/common/`, `dev/` is `intel/dev/`, `iris/` is `gallium/drivers/iris/`,
`anv/` is `intel/vulkan/`. The i915 backends are `iris/i915/{iris_batch,iris_bufmgr,iris_kmd_backend}.c`,
`anv/i915/{anv_batch_chain,anv_device,anv_gem,anv_kmd_backend,anv_queue}.c`, `common/i915/{intel_gem,intel_engine}.c`,
`dev/i915/intel_device_info.c`. The generic wrappers are `common/intel_gem.c` (dispatch on `kmd_type`) and `dev/intel_kmd.c`.

The Mesa tree carries CoralOS edits that change what is asked of the kernel side, called out where they matter:
`dev/intel_device_info.c:1896-1928` (`#ifdef __coral__`: no sysfs, so the PCI id comes from `GETPARAM(CHIPSET_ID)`, and `gtt_size` /
`aperture_bytes` are seeded in userspace), `iris/i915/iris_kmd_backend.c:317-362` (a sparse, open-addressed handle table because
coral-drm mints generation-tagged handles) and the `iris_heap_probe` / `iris_coral_mark` markers.

## Legend

| Status | Meaning |
|---|---|
| **REQ** | Mesa fails device creation, or renders wrong, if this is absent or answers an error. |
| **OPT(fb)** | Optional with a fallback; `fb` is the fallback Mesa takes. The port may answer the Linux error and Mesa still works. |
| **G12+** | Only issued (or only meaningful) on Gen12+/discrete parts; never reached on a Gen9 iGPU. |
| **PROT** | Protected content (PXP): Gen12+ hardware; Linux answers `-ENODEV`, Mesa treats that as "unsupported". |
| **REFUSED** | The port deliberately refuses (see plan section 2: userptr until revocable pins exist; perf/OA). Mesa tolerates it. |

"Port answer" names the crate that owns the semantics. `i915-uapi-core` (this crate) owns the decode / validate / reply of GETPARAM,
I915_QUERY and EXECBUFFER2 and the data types; GEM object / context / VM / mmap lifetime is `i915-gem-core`; syncobj / PRIME /
sync_file state is the `i915.device` session tables (generic DRM semantics, `drm_syncobj.c` / `drm_prime.c`).

## 0. Device bring-up order (what runs, in order, for one process)

1. `drmGetVersion` -> `DRM_IOCTL_VERSION` (`dev/intel_kmd.c:35`; libdrm `xf86drm.c:1383,1395`): `name == "i915"` selects the i915 backend.
   **REQ**.
2. `intel_get_device_info_from_fd` (`dev/intel_device_info.c:1859`; iris `iris/iris_bufmgr.c:2725` with min gen 8, anv
   `anv/anv_physical_device.c:2773` with min gen 9). CoralOS: `GETPARAM(CHIPSET_ID)` (`:1906-1908`). Then `intel_device_info_i915_get_info_from_fd`
   (`dev/i915/intel_device_info.c:561-644`): the queries of section 1 and 3.
3. anv only: `anv_i915_physical_device_get_parameters` (`anv/i915/anv_device.c:68-148`): eight REQ GETPARAMs, dma-buf sync_file probe, priority probe.
4. iris only: `iris_detect_kernel_features` (`iris/iris_screen.c:588-596`): syncobj `WAIT_FOR_SUBMIT` probe, protected-context probe;
   `has_context_isolation` gate (`iris/iris_screen.c:717-727`, "Checking the last feature availability will include all previous ones").
5. Per context / queue: sections 2, 4, 5.

## 1. DRM_IOCTL_I915_GETPARAM

`i915-uapi-core::getparam` answers all 59 selectors. Linux value for **0x591C GT2** in the last column (tests/getparam.rs pins each).

| Selector (value) | Mesa call site(s) | Status | Gen9 notes | Linux value, 0x591C |
|---|---|---|---|---|
| `CHIPSET_ID` (4) | `common/i915/intel_gem.c:313` (`can_render_on_fd`, `val > 0`); CoralOS `dev/intel_device_info.c:1906-1908` | **REQ** | CoralOS device-id source; no sysfs | `0x591c` |
| `REVISION` (32) | `dev/i915/intel_device_info.c:574` | OPT(revision = 0) | | PCI revision |
| `CS_TIMESTAMP_FREQUENCY` (51) | `dev/i915/intel_device_info.c:567` | **REQ** for ver >= 10; **OPT(keep table value)** on Gen9 | timestamp scaling uses it when present | `12000000` (24 MHz crystal >> 1) |
| `SLICE_MASK` (46) / `SUBSLICE_MASK` (47) / `EU_TOTAL` (34) | `dev/i915/intel_device_info.c:271,275,279` | OPT(fb): only used when the topology query fails (`:586`); Mesa wants ONE of the two paths | | `1` / `0b111` / `24` |
| `MMAP_GTT_VERSION` (40) | `dev/i915/intel_device_info.c:624` | OPT(fb): `>= 4` enables MMAP_OFFSET, `>= 5` partial mmap; else legacy `GEM_MMAP` | | `5` |
| `HAS_USERPTR_PROBE` (56) | `dev/i915/intel_device_info.c:628` | OPT | Linux answers 1 unconditionally. userptr itself is REFUSED by the port; `has_userptr_uapi` (`:500-511`) is decided by the ioctl, not this param | `1` |
| `HAS_CONTEXT_ISOLATION` (50) | `dev/i915/intel_device_info.c:630`; gate `iris/iris_screen.c:726` | **REQ (iris)**: zero = "Kernel is too old" | bitmask of engine classes with a default context state | `0xf` |
| `<result of the previous getparam>` | `dev/i915/intel_device_info.c:632` | Mesa bug | `getparam(fd, getparam(fd, HAS_CONTEXT_ISOLATION, &val), &val)` passes the boolean 0/1 as the selector: selector 1 = `IRQ_ACTIVE`, Linux `-ENODEV` (the intent was probably `HAS_CONTEXT_FREQ_HINT`, 59). Harmless: `supports_low_latency_hint` stays false | `-ENODEV` |
| `PXP_STATUS` (58) | `common/i915/intel_gem.c:281`; `iris/i915/iris_batch.c:107,185` (`wait_on_get_param`, only when the app asks for a protected context) | PROT | `errno == ENODEV` => unsupported | `-ENODEV` |
| `HAS_WAIT_TIMEOUT` (19) | `anv/i915/anv_device.c:75` | **REQ (anv)** | | `1` |
| `HAS_EXECBUF2` (9) | `anv/i915/anv_device.c:81` | **REQ (anv)** | | `1` |
| `MMAP_VERSION` (30) | `anv/i915/anv_device.c:88` | **REQ (anv)** only when `!has_llc` (Atom); not asked on KBL | | `1` |
| `HAS_EXEC_SOFTPIN` (37) | `anv/i915/anv_device.c:94` | **REQ (anv)** | iris relies on it (every object `PINNED`) | `1` |
| `HAS_EXEC_FENCE_ARRAY` (49) | `anv/i915/anv_device.c:100`; iris relies on it (comment `iris/iris_screen.c:720`) | **REQ** | | `1` |
| `HAS_EXEC_ASYNC` (43) | `anv/i915/anv_device.c:106` | **REQ (anv)** | `EXEC_OBJECT_ASYNC` | `1` |
| `HAS_EXEC_CAPTURE` (45) | `anv/i915/anv_device.c:112` | **REQ (anv)** | `EXEC_OBJECT_CAPTURE` | `1` |
| `HAS_EXEC_TIMELINE_FENCES` (55) | `anv/i915/anv_device.c:118` | **REQ (anv)** | timeline-fences execbuf extension | `1` |
| `HAS_EXEC_NO_RELOC` (25), `HAS_EXEC_HANDLE_LUT` (26), `HAS_EXEC_BATCH_FIRST` (48) | not queried; iris relies on them (`iris/iris_screen.c:717-721`) | **REQ (iris)**, implied by `HAS_CONTEXT_ISOLATION` | | `1` |
| `NUM_FENCES_AVAIL` (6), `HAS_ALIASING_PPGTT` (18), `HAS_LLC` (17), `HAS_EXEC_*` rest | libdrm_intel only (`intel_bufmgr_gem.c:3661,3687,3640`), section 7 | legacy | | `32`, `2`, `1` |

All other selectors (`HAS_SCHEDULER`, `HAS_GPU_RESET`, `CMD_PARSER_VERSION`, `HUC_STATUS`, `HAS_CONTEXT_FREQ_HINT`, ...) are not issued by iris or
anv; the port answers them as Linux does (`PERF_REVISION` / `OA_TIMESTAMP_FREQUENCY` as unknown, perf not ported).
`DRM_IOCTL_I915_GETPARAM` is wrapped by `intel_ioctl` (`common/intel_gem.h:81-95`) which **retries on `EINTR`/`EAGAIN`**: a port that answers
`EAGAIN` for "busy" turns Mesa into a spin loop.

## 2. Contexts: CREATE / CREATE_EXT / DESTROY / GETPARAM / SETPARAM / VM

Context id 0 is the file's default (proto) context: Mesa issues `GETPARAM` / `SETPARAM` on **id 0** before it ever creates a context
(`dev/i915/intel_device_info.c:615`, `iris/i915/iris_bufmgr.c:65`, `anv/i915/anv_device.c:64,144`); Linux serves it from
`i915_gem_context_open` (`gem/i915_gem_context.c:1742-1759`, `:2609-2623`).

| Call | Mesa call site(s) | Status | Detail |
|---|---|---|---|
| `GEM_CONTEXT_CREATE` | `common/i915/intel_gem.c:38`; iris `iris/i915/iris_batch.c:119`; anv `anv/i915/anv_device.c:311` | OPT(fb) | the legacy path (no engine map). iris tries the engines context first (`iris/i915/iris_batch.c:305-309`) and falls back to per-batch contexts (`:132-148`) |
| `GEM_CONTEXT_CREATE_EXT` with `USE_EXTENSIONS` | `common/i915/intel_gem.c:147-190` (engines), `:255-266` (protected probe) | OPT(fb) for iris; **REQ** for anv when `has_vm_control` | extension chain of `I915_CONTEXT_CREATE_EXT_SETPARAM`: `ENGINES` (`:120-128`), `RECOVERABLE` (`:135-140`), `VM` when `vm_id != 0` (`:150-168`), `PROTECTED_CONTENT` (PROT), `LOW_LATENCY` (G12+/GuC). Mesa retries on `EIO` only when protected (`:184-190`). Plan: engine map of N slots of class RENDER (Gen9: `RCS0`; iris asks `COPY` for the blitter only on ver >= 12, `iris/i915/iris_batch.c:166-173`) |
| `GEM_CONTEXT_DESTROY` | `common/i915/intel_gem.c:50` | **REQ** | iris `iris_batch.c:252`; anv `anv_queue.c:105,125`, `anv_device.c:332` |
| `GEM_CONTEXT_SETPARAM` `PRIORITY` | iris `iris/i915/iris_batch.c:55`; anv `anv/i915/anv_device.c:52-65,257` | OPT | anv probes LOW..REALTIME on ctx 0 (`anv_device.c:63-66,139-141`); errors lower `max_context_priority`. Value range: `I915_CONTEXT_MIN/MAX_USER_PRIORITY` = -1023..1023 |
| `GEM_CONTEXT_SETPARAM` `RECOVERABLE = 0` | iris `iris/i915/iris_batch.c:81`; anv `anv/i915/anv_device.c:244`; also in CREATE_EXT | **REQ** (semantics): "report the context lost on the next submit, do not replay" | |
| `GEM_CONTEXT_SETPARAM` `VM` | iris `iris/i915/iris_batch.c:91` | OPT(fb) | binds the context to the global VM id obtained from ctx 0 |
| `GEM_CONTEXT_GETPARAM` `VM` | iris `iris/i915/iris_bufmgr.c:65`; anv `anv/i915/anv_device.c:144` | OPT(fb): absence => no VM control, anv uses the single-context path | the value is the VM id to pass back in `SETPARAM`/`CREATE_EXT` |
| `GEM_CONTEXT_GETPARAM` `GTT_SIZE` | `dev/i915/intel_device_info.c:615` | OPT(fb): left 0; **iris refuses to start with `gtt_size <= 12 GB`** (CoralOS seeds it, `dev/intel_device_info.c:1920-1925`) | Gen9 full PPGTT: `1 << 48` |
| `GEM_CONTEXT_GETPARAM` `PROTECTED_CONTENT` | iris `iris/i915/iris_batch.c:227` | PROT | |
| `GEM_VM_CREATE` / `GEM_VM_DESTROY` | anv `anv/i915/anv_device.c:409` / `:402` | **REQ (anv)** on the `has_vm_control` path | |
| `GET_RESET_STATS` | iris `iris/i915/iris_kmd_backend.c:282`; anv `anv/i915/anv_device.c:343` | **REQ** (device-lost detection) | `batch_active` / `batch_pending` => guilty / innocent reset; an error is only logged by iris |

Engine classes used on Gen9: `I915_ENGINE_CLASS_RENDER` (iris and anv), `COPY` (anv queue family, `anv/i915/anv_queue.c:47`),
`VIDEO` (anv video queue, `:54`: legacy `I915_EXEC_BSD | I915_EXEC_BSD_RING1`), `VIDEO_ENHANCE` (mapped, never created). **G12+**:
`COMPUTE` (`common/i915/intel_engine.c:42,62`), iris blitter engine (`iris/i915/iris_batch.c:166-173`), companion RCS context (`anv/i915/anv_queue.c:80-93`).
`I915_CONTEXT_PARAM_ENGINES` is an extension-carrying struct (`I915_DEFINE_CONTEXT_PARAM_ENGINES`): the engine table is a **flattened array** on the wire;
`SSEU`, `PERSISTENCE`, `RINGSIZE`, `NO_ERROR_CAPTURE`, `BANNABLE`, `BAN_PERIOD`, `CONTEXT_IMAGE` are not issued.

## 3. DRM_IOCTL_I915_QUERY

Always the two-call protocol of `intel_i915_query_alloc` (`length = 0` probe, then fetch): the probe returns the size, an error is the item's negative `length`.

| Query id | Mesa call site(s) | Status | Gen9 answer (port: `i915-uapi-core::query`) |
|---|---|---|---|
| `TOPOLOGY_INFO` (1) | `dev/i915/intel_device_info.c:301` | OPT(fb): on failure ver < 10 falls back to the GETPARAM masks (`:586`) | max_slices 3, max_subslices 4, max_eus 8; 32 bytes for the 24-EU GT2 (byte-exact vector in tests/query.rs) |
| `ENGINE_INFO` (2) | `common/i915/intel_engine.c:72` (iris `iris/i915/iris_batch.c:155`, anv `anv/anv_physical_device.c:2996`) | OPT(fb): NULL => iris uses non-engine contexts, anv legacy rings | `RCS0, BCS0, VCS0 (HEVC\|SFC), VECS0`; header must be zero |
| `MEMORY_REGIONS` (4) | `dev/i915/intel_device_info.c:333` | OPT(fb): falls back to `os_*` RAM numbers, `use_class_instance = false` => legacy `GEM_CREATE` | one region: `SYSTEM`, instance 0 |
| `HWCONFIG_BLOB` (5) | `dev/i915/intel_device_info.c:558` | OPT | `-ENODEV` |
| `GEOMETRY_SUBSLICES` (6) | `dev/i915/intel_device_info.c:307` | **G12+** (`verx10 >= 125` only) | `-ENODEV` |
| `GUC_SUBMISSION_VERSION` (7) | `common/i915/intel_engine.c:104` | **G12+** in effect (`i915_engines_is_guc_semaphore_functional`) | `-ENODEV` |
| `PERF_CONFIG` (3) | `intel/perf/*` only (performance-query layers) | REFUSED (perf not ported) | `-ENODEV` |

## 4. GEM object ioctls

| Call | Mesa call site(s) | Status | Detail |
|---|---|---|---|
| `GEM_CREATE_EXT` | iris `iris/i915/iris_kmd_backend.c:135`; anv `anv/i915/anv_kmd_backend.c:126` | **REQ** once `MEMORY_REGIONS` succeeded (`use_class_instance`) | extension `MEMORY_REGIONS` (`iris_kmd_backend.c:88-95`, `anv_kmd_backend.c:105-108`): **SYSTEM class only** on Gen9. `flags NEEDS_CPU_ACCESS` (`iris :113`, `anv :95`): VRAM small-BAR only. `PROTECTED_CONTENT` ext (`iris :121`, `anv :122`): PROT. `SET_PAT` ext (`iris :131`, `anv :115`): only when `has_set_pat_uapi` (ver > 12 / MTL), never on Gen9 |
| `GEM_CREATE` | iris `iris/i915/iris_kmd_backend.c:72`; anv `anv/i915/anv_kmd_backend.c:59`; probes `dev/i915/intel_device_info.c:431,481`, `common/i915/intel_gem.c:325` | **REQ** (the fallback when MEMORY_REGIONS is absent; also used by probes) | `size`, returns `handle`; anv uses the returned `size` |
| `GEM_CLOSE` (DRM core) | iris `iris/i915/iris_kmd_backend.c:466`, `iris/iris_bufmgr.c:1368`; anv `anv/i915/anv_kmd_backend.c:158`; probes `dev/i915/intel_device_info.c:467,494`, `common/i915/intel_gem.c:361` | **REQ** | close does not revoke an outstanding mapping (plan section 2) |
| `GEM_SET_DOMAIN` | iris `iris/i915/iris_kmd_backend.c:52` (called `:144` right after create with `CPU` read domain, to pre-fault pages) | OPT | semantically a no-op for coherence on LLC parts; must not fail |
| `GEM_MMAP_OFFSET` | iris `iris/i915/iris_kmd_backend.c:216`; anv `anv/i915/anv_kmd_backend.c:170` | **REQ** when `MMAP_GTT_VERSION >= 4` | flags `WB` (2) / `WC` (1) / `UC` (3) on integrated parts; `FIXED` (4) only with local memory (**G12+**). The returned offset is then `mmap()`ed on the DRM fd (`iris :230`, `anv :192`): a **shmem lease**, not a page fault (plan section 2) |
| `GEM_MMAP` | iris `iris/i915/iris_kmd_backend.c:254`; anv `anv/i915/anv_kmd_backend.c:218` | OPT(fb): only when `!has_mmap_offset` | not reached when `MMAP_GTT_VERSION` is 5 |
| `GEM_WAIT` | iris `iris/i915/iris_bufmgr.c:56`; anv `anv/i915/anv_gem.c:83` | **REQ** | `timeout_ns` in/out; `-ETIME` is the timeout result |
| `GEM_BUSY` | iris `iris/i915/iris_bufmgr.c:40` | **REQ** (idle tracking) | an error is read as "not busy" |
| `GEM_MADVISE` | iris `iris/i915/iris_kmd_backend.c:162` | OPT | `WILLNEED` / `DONTNEED`; failure only warns; `retained` result |
| `GEM_SET_CACHING` | iris `iris/i915/iris_kmd_backend.c:176` (caller `iris/iris_bufmgr.c:1341`); anv `anv/i915/anv_kmd_backend.c:42` | OPT: non-LLC parts (Atom) only | `has_caching_uapi` is true on Gen9 (`dev/i915/intel_device_info.c:619`) but KBL has LLC so it is not exercised |
| `GEM_SET_TILING` / `GEM_GET_TILING` | iris `iris/i915/iris_bufmgr.c:103` / `:76`; anv `anv/i915/anv_gem.c:70` / `:43`; probes `dev/i915/intel_device_info.c:447,456,489` | OPT: `has_tiling_uapi` gates the use; X / Y tiling modes for shared (scanout) buffers. The bit-6 swizzle probe runs only for ver < 8 | `swizzle_mode` NONE on Gen9 |
| `GEM_USERPTR` | iris `iris/i915/iris_kmd_backend.c:478`; anv `anv/i915/anv_kmd_backend.c:316`; probe `dev/i915/intel_device_info.c:508` | REFUSED | probe expects `EINVAL` from a zero-size request to set `has_userptr_uapi`; a different errno just disables `EXT_external_memory_host` and `resource_from_user_memory` |
| `GET_APERTURE` | `dev/i915/intel_device_info.c:415` | OPT: `aperture_bytes` left 0 (CoralOS seeds 256 MiB) | |
| `REG_READ` | `common/i915/intel_gem.c:232` (offset `RCS_TIMESTAMP`, flag `I915_REG_READ_8B_WA`); iris `iris/iris_screen.c:515` (`get_timestamp`); anv `anv/anv_physical_device.c:2925` | OPT: `has_reg_timestamp` / zero timestamp | a one-register allow-list (the render timestamp, 0x2358); owned by uncore, not by this crate |
| `GEM_FLINK` / `GEM_OPEN` (DRM core) | iris `iris/iris_bufmgr.c:2190` / `:1482` | OPT: flink-name sharing (`DRM_API_HANDLE_TYPE_SHARED`) | |
| `GEM_VM_BIND`, sparse | iris `iris/i915/iris_kmd_backend.c:444-458`, anv `anv/i915/anv_kmd_backend.c` (`i915_vm_bind` returns success) | not issued | i915 has no VM_BIND on Gen9; both backends no-op |

## 5. DRM_IOCTL_I915_GEM_EXECBUFFER2

Wrapper `i915_gem_execbuf_ioctl` (`common/i915/intel_gem.h:146-169`): asserts `FENCE_OUT` is never set (`:151`), and retries on `ENOMEM` up to 80 times
with a quadratic sleep (`:156-166`, `common/intel_gem.h:147-153`). `DRM_IOCTL_I915_GEM_EXECBUFFER2_WR` is never issued by iris or anv (libdrm_intel only).

| Field / flag | iris | anv | Status |
|---|---|---|---|
| `buffers_ptr`, `buffer_count` | `iris/i915/iris_kmd_backend.c:401-402` (deduplicated list; writes OR the `WRITE` flag, `:344`) | `anv/i915/anv_batch_chain.c:524-525,557-558,679-680` | **REQ** |
| `batch_start_offset` | 0 (`:404`) | `calc_batch_start_offset` (`anv_batch_chain.c:377`; used `:526,559`) | **REQ** |
| `batch_len` | `align(primary_batch_size, 8)` (`:405`) | 0 (= to the end, `:527`), 8 for the empty batch (`:560`) | **REQ**: 8-byte alignment is a Linux check |
| `rsvd1` | context id (`:410`) | context id (`:535,562`) | **REQ**: low 32 bits |
| `rsvd2` | 0 | 0 (`:536,563`) | in-fence fd lives here: never used |
| `num_cliprects` / `cliprects_ptr` | the `drm_i915_gem_exec_fence` array under `FENCE_ARRAY` (`:413-417`) | same, `:579-582`; or the extension chain head under `USE_EXTENSIONS` (`:72-80`) | **REQ**; the two are mutually exclusive in Linux |
| `I915_EXEC_RENDER` (1) / `I915_EXEC_BLT` (3) | `iris/i915/iris_batch.c:138,143` (non-engine contexts) | `anv/i915/anv_queue.c:47,50` (legacy engines) | OPT(fb): ring selector when no engine map |
| `I915_EXEC_BSD` (2) `\| I915_EXEC_BSD_RING1` | | `anv/i915/anv_queue.c:54` | OPT (video queue family) |
| ring index = engine slot | `iris/i915/iris_batch.c:216` (`exec_flags = i` with an engines context) | `anv/i915/anv_batch_chain.c:321` (0 with `has_vm_control`) | OPT(fb) |
| `I915_EXEC_NO_RELOC` (1<<11) | `iris :407` | `anv_batch_chain.c:532,561,682` | **REQ**; also the `__EXEC_HAS_RELOC` write-back switch in Linux |
| `I915_EXEC_HANDLE_LUT` (1<<12) | `iris :409` | `anv_batch_chain.c:533,561,683,977` | **REQ**: relocation targets are list indices |
| `I915_EXEC_BATCH_FIRST` (1<<18) | `iris :408` (batch is object 0) | not used (batch is the last object) | **REQ (iris)**; Linux batch position: `is_batch_buffer` |
| `I915_EXEC_FENCE_ARRAY` (1<<19) | `iris :414` | `anv_batch_chain.c:580` (binary syncobjs) | **REQ** |
| `I915_EXEC_USE_EXTENSIONS` (1<<21) + `DRM_I915_GEM_EXECBUFFER_EXT_TIMELINE_FENCES` | | `anv_batch_chain.c:72,577` (timeline syncobjs: handles array + `values_ptr`) | **REQ (anv)** |
| `I915_EXEC_FENCE_IN` / `FENCE_OUT` / `FENCE_SUBMIT` | not set (`FENCE_OUT` asserted off) | not set | not issued; still decoded and validated |
| `I915_EXEC_SECURE`, `IS_PINNED`, `GEN7_SOL_RESET`, `CONSTANTS_*`, `RESOURCE_STREAMER` | not set | not set | refused / ignored as Linux does |
| `exec_object2.handle`, `offset` | `bo->address` (`:357`; the softpin address) | `bo->offset` (`anv_batch_chain.c:138`) | **REQ** |
| `exec_object2.flags` `PINNED` (1<<4) \| `SUPPORTS_48B_ADDRESS` (1<<3) | `:346` (every object) | `anv/i915/anv_kmd_backend.c:327-330` | **REQ**: the softpin contract |
| `EXEC_OBJECT_WRITE` (1<<2) | `:350` (written objects) | `anv_kmd_backend.c:338`; `anv_batch_chain.c:145-147` | **REQ** |
| `EXEC_OBJECT_ASYNC` (1<<6) | `:348-349` (every non-external object) | `anv_kmd_backend.c:342` | **REQ** (implicit-sync opt-out) |
| `EXEC_OBJECT_CAPTURE` (1<<7) | `:347` | `anv_kmd_backend.c:334` | OPT |
| `relocation_count`, `relocs_ptr`, `alignment`, `rsvd1/2` | 0 | 0 (`anv_batch_chain.c:135-142`) | relocations are **not** issued by iris or anv on i915; the port implements them anyway (plan section 2) |
| `exec_fence.handle` / `flags` `WAIT` (1<<0) `\|` `SIGNAL` (1<<1) | `iris/iris_fence.c:98` | `anv_batch_chain.c:224-231,255-258` | **REQ** |

## 6. Syncobj, timeline, sync_file, dma-buf, PRIME (generic DRM / kernel ABI)

iris calls the ioctls directly; anv goes through `vulkan/runtime/vk_drm_syncobj.c` -> `util_sync_provider` (`util/u_sync_provider.c:25-103`) -> libdrm
`drmSyncobj*` (`xf86drm.c:4984-5212`). anv **asserts timeline + CPU-wait features** (`anv/anv_physical_device.c:2907-2909`) and the type is probed by
creating a SIGNALED syncobj and waiting with the timeline flags (`vulkan/runtime/vk_drm_syncobj.c:659-705`).

| Ioctl (flags used) | iris | anv (`vk_drm_syncobj.c` line of the provider call) | Status |
|---|---|---|---|
| `SYNCOBJ_CREATE` (`0`, `SIGNALED`) | `iris/iris_fence.c:29`; probe `common/intel_gem.c:39` | `:58,579,662` | **REQ** |
| `SYNCOBJ_DESTROY` | `iris/iris_fence.c:41`; `common/intel_gem.c:56` | `:83,380,592,614,701` | **REQ** |
| `SYNCOBJ_WAIT` (`WAIT_ALL`, `WAIT_FOR_SUBMIT`) | `iris/iris_fence.c:210,459`; `iris/iris_bufmgr.c:562`; probe `common/intel_gem.c:51` | `:227,347,687` | **REQ** (`WAIT_FOR_SUBMIT` = kernel 5.2 semantics) |
| `SYNCOBJ_SIGNAL` | `iris/iris_fence.c:81` | `:98,139` | **REQ** |
| `SYNCOBJ_RESET` | | `:177,199` | **REQ (anv)** |
| `SYNCOBJ_TIMELINE_WAIT` (`WAIT_AVAILABLE`, `WAIT_FOR_SUBMIT`, `WAIT_ALL`) | `iris/iris_fence.c:305` | `:336,342` | **REQ**: the kernel never implemented `WAIT_AVAILABLE` for binary (`vk_drm_syncobj.c:290`) |
| `SYNCOBJ_TIMELINE_SIGNAL` | | `:65,96,135` | **REQ (anv)** |
| `SYNCOBJ_QUERY` | | `:162` | **REQ (anv)** (timeline value) |
| `SYNCOBJ_TRANSFER` | | `:560,589,603` (copy payloads between timelines) | **REQ (anv)** |
| `SYNCOBJ_HANDLE_TO_FD` (opaque; `EXPORT_SYNC_FILE`) | `iris/iris_fence.c:507,522`; `iris/iris_batch.c:860` | `:395,217,427` | **REQ**: a syncobj / sync_file fd needs dup / close / poll / import / export in relibc |
| `SYNCOBJ_FD_TO_HANDLE` (opaque; `IMPORT_SYNC_FILE`) | `iris/iris_fence.c:548`; `iris/iris_bufmgr.c:475` | `:374,411` | **REQ** |
| `SYNCOBJ_EVENTFD` | | libdrm `xf86drm.c:5212`; not called by anv/iris | not issued |
| `SYNC_IOC_MERGE` (sync_file fd) | `iris/iris_fence.c:477` | `vulkan/runtime/vk_drm_syncobj.c:496` (merged in userspace via `sync_merge`) | **REQ** (fd-merge semantics) |
| `DMA_BUF_IOCTL_EXPORT_SYNC_FILE` (`DMA_BUF_SYNC_RW`) | `iris/iris_bufmgr.c:460` | probe `common/i915/intel_gem.c:344` | **REQ (anv)** (`anv_device.c:123`): implicit-sync bridge |
| `DMA_BUF_IOCTL_IMPORT_SYNC_FILE` | `iris/iris_bufmgr.c:500` (`DMA_BUF_SYNC_WRITE`) | probe `common/i915/intel_gem.c:352` | **REQ (anv)** |
| `PRIME_HANDLE_TO_FD` (`DRM_CLOEXEC \| DRM_RDWR`) | `iris/iris_bufmgr.c:1444,2158`; probe `common/i915/intel_gem.c:334` | `anv/anv_gem.c:92` | **REQ** |
| `PRIME_FD_TO_HANDLE` | `iris/iris_bufmgr.c:2003`, `drmPrimeFDToHandle` `:2243` | `anv/anv_gem.c:106` | **REQ** |
| intel bind timeline (`SYNCOBJ_CREATE`/`TIMELINE_WAIT`/`DESTROY`) | | `common/intel_bind_timeline.c:15,44,45`; user `anv/anv_perf.c:139` | perf-query only: OPT / REFUSED |

Generic DRM calls with no i915 parameters but still on the path: `DRM_IOCTL_VERSION` (section 0), `DRM_IOCTL_GEM_CLOSE`, `DRM_IOCTL_GEM_OPEN`,
`DRM_IOCTL_GEM_FLINK`. `drmGetDevice2` (PCI id / revision) is bypassed by the CoralOS patch (`dev/intel_device_info.c:1896`);
neither driver calls `DRM_IOCTL_GET_CAP`.

## 7. libdrm `intel/` (libdrm_intel): not on the iris / anv path

`libdrm/intel/intel_bufmgr_gem.c` is the legacy bufmgr used by igt and old tools, not by modern Mesa. It issues, beyond sections 1-6:
`GEM_PREAD` (`:1849`) / `GEM_PWRITE` (`:1788`), `GEM_PIN` (`:2457`) / `GEM_UNPIN` (`:2478`) (removed from Linux), `GEM_MMAP_GTT` (`:1502,3422`),
`GEM_SW_FINISH` (`:1667`), `GEM_GET_CACHING` (`:1701`), `GET_PIPE_FROM_CRTC_ID` (`:1816`), `EXECBUFFER2_WR` (`:2370`), `GEM_THROTTLE`, and GETPARAMs
`HAS_BSD/BLT/VEBOX/LLC/RELAXED_FENCING/EXEC_ASYNC/WAIT_TIMEOUT/EXEC_SOFTPIN/NUM_FENCES_AVAIL/ALIASING_PPGTT/SUBSLICE_TOTAL/EU_TOTAL/HAS_POOLED_EU/MIN_EU_IN_POOL`
(`:3277-3688`). The port does not serve these; igt / libdrm_intel based tools are out of scope until a consumer needs them.

## 8. What the port must answer: the frozen minimum (Gen9)

Cross-checked against plan section 2's list. **Confirmed**: GETPARAM, GEM_CREATE(_EXT, system region), GEM_MMAP_OFFSET, GEM_CLOSE, CONTEXT_CREATE(_EXT),
CONTEXT_DESTROY, CONTEXT GETPARAM/SETPARAM (VM, priority, recoverable), GEM_VM_CREATE/DESTROY, EXECBUFFER2 (softpin, FENCE_ARRAY, timeline extension, BATCH_FIRST,
NO_RELOC), GEM_WAIT, GEM_BUSY, GEM_MADVISE, GET_RESET_STATS, I915_QUERY (topology / engine info / memory regions), syncobj + timeline, PRIME.

**Present in Mesa but missing from the plan's list (add to the minimum):**
1. `DRM_IOCTL_VERSION` answering `"i915"`.
2. `GEM_SET_DOMAIN` (called after every create on non-VRAM parts).
3. `GEM_GET_TILING` / `GEM_SET_TILING` (gated by `has_tiling_uapi`; needed for X / Y tiled shared buffers).
4. `REG_READ` of the render timestamp (`iris` `get_timestamp`; anv `has_reg_timestamp`).
5. `DMA_BUF_IOCTL_EXPORT_SYNC_FILE` / `IMPORT_SYNC_FILE` (anv refuses to start without them) and `SYNC_IOC_MERGE`.
6. `GEM_FLINK` / `GEM_OPEN` (flink-name sharing) and `GEM_MMAP` (only if `MMAP_GTT_VERSION < 4`).
7. Context id 0 as a default context accepting GETPARAM / SETPARAM, and `CONTEXT_PARAM_GTT_SIZE` / `APERTURE` (or Mesa-side seeding, which CoralOS already does).
8. `SYNCOBJ_TRANSFER`, `SYNCOBJ_QUERY`, `SYNCOBJ_RESET`, `SYNCOBJ_TIMELINE_SIGNAL` (anv only).

**In the plan but not issued by iris / anv on Gen9:** relocations (`relocation_count` is always 0), `GEM_SET_CACHING` on KBL (LLC), `GEM_USERPTR` (only a probe), `GEM_VM_BIND`,
`GEM_MMAP` (when `MMAP_GTT_VERSION` is 5).

**Quirks the port must reproduce faithfully (each has a test in `i915-uapi-core`):**
- `GETPARAM(HAS_CONTEXT_FREQ_HINT)` succeeds with the VALUE `-EINVAL` (`i915_getparam.c:161-166`).
- `HUC_STATUS` is `-EOPNOTSUPP` (HuC disabled), `PXP_STATUS` is `-ENODEV`: Mesa matches `ENODEV` only.
- `I915_QUERY` item `length` is compared signed-vs-unsigned (`i915_query.c:24`): a negative length passes.
- `EXECBUFFER2` `DR4 == 0xffffffff` is repaired to zero (`i915_gem_execbuffer.c:2193-2196`).
- Under full PPGTT (Gen9), `EXEC_OBJECT_NEEDS_GTT` is invalid (`i915_gem_execbuffer.c:833`).
- A reloc whose `presumed_offset` already matches skips all offset checks (`i915_gem_execbuffer.c:1472-1477`).
- `Mesa` bug `dev/i915/intel_device_info.c:632` (selector = a boolean) is harmless: answer `-ENODEV` for selector 1.

## 9. Mapping to crates

| Area | Crate / owner |
|---|---|
| i915_drm.h constants and data types | `i915-uapi-core::{consts, types, gem_types}` |
| GETPARAM answers | `i915-uapi-core::getparam` (inputs: `PlatformDescriptor`, SSEU, CS clock, RAM, GT facts via `UapiDevice`) |
| I915_QUERY answers | `i915-uapi-core::query` |
| EXECBUFFER2 decode + validation | `i915-uapi-core::{execbuf_wire, execbuf}`; execution, relocation patching, pinning: `i915-gem-core` + `i915-submit-core` |
| Context / VM / object / mmap-lease tables, `ExecEnv` implementation | `i915-gem-core` + `devices/i915` |
| syncobj / timeline / sync_file / PRIME state | `devices/i915` session tables (generic `drm_syncobj.c` / `drm_prime.c` semantics) |
| `REG_READ` allow-list | `i915-uncore-core` |
