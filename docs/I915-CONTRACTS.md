# i915 port family: the frozen contracts (CONTRACT_ABI = 1)

Written 2026-10-12. `i915-common-core`'s header cites this file; it had not been committed. Everything below is a description of
code that exists in this repo; where a statement and the code disagree, the code wins and this file is the bug.

CONTRACT_ABI = 1 (`i915_common_core::CONTRACT_ABI`). Any change to a frozen shape below bumps it, amends this file in the same
commit, and changes the marker the device prints: `[I915-CONTRACT abi=1 ...]`. Additive, non-breaking growth (a new
provided trait method, a new refusal variant that no existing `match` on a foreign type could be exhaustive over) is listed in
"Additive changes" at the end and does NOT bump the ABI.

## 0. Shape rules every crate in the family obeys

* Pure `no_std`, `#![forbid(unsafe_code)]`, no MMIO of its own. A port crate is a decoder, an encoder, a planner or a state machine:
  values in, values / effect lists out. Only `devices/i915` executes effects, through `coralos-hal` and `coralos-rt`.
* One home per register: the crate named for the Linux header that defines it. Others import the constant and never re-type an
  offset. (Exceptions are recorded in the importing crate's `regs.rs` doc with a "move me" note until the home crate has the row.)
* Every bound a crate declares (poll cap, retry count, table capacity, op buffer, victim cap) is enforced and has a test that fails
  if the check is removed.
* Every failure has a stable, grep-able name. No bare `false`, no silent clamp.
* Files under 1000 lines; GPL-2.0-only for ported code, with `file:line` on every cited constant (`tools/check-citations.sh`).
* Plans are DATA. A planner returns what to do; it never does it. A plan the executor cannot apply is not "rolled back in hardware":
  the model that produced it must be rebuilt.

## 1. Typed registers: `Reg`, `RegIo` (`i915-common-core::reg`)

* `Reg(u32)` is an offset into BAR0 (Linux `i915_reg_t`, `_MMIO(x)`). 32-bit registers only. `Reg::new`, `Reg::offset`,
  `Reg::nth(base, n, stride)`.
* Field helpers, all `const`: `bit(n)`, `genmask(hi, lo)`, `field_get(mask, v)`, `field_prep(mask, v)`, `masked_field`,
  `try_masked_field`, `masked_bit_enable/disable` (the "masked register" write encoding: high half selects which low bits change).
* `RegIo` (`rd(Reg) -> u32`, `wr(Reg, u32)`) is implemented for every `coralos_hal::Mmio`. It is the only way a port reads a register.
* `coralos_hal::Mmio` is the one MMIO trait: `r8/r16/r32`, `w8/w16/w32`, plus `relax`, `arm_deadline`, `deadline_expired`, `park_ms`
  (defaults are inert; a host that has a clock overrides them so a wait is bounded in wall-clock time).

## 2. `I915Error` (`i915-common-core::error`)

`Copy`, no allocation. Variants (name token from `I915Error::name()` in parentheses): `UnknownDevice` (UNKNOWN-DEVICE),
`UnsupportedGeneration` (UNSUPPORTED-GENERATION), `WriteRefused` (WRITE-NOT-ALLOWED), `AckTimeout` (ACK-TIMEOUT), `MmioUnreliable`
(MMIO-UNRELIABLE), `NoSuchDomain` (NO-SUCH-DOMAIN), `Unbalanced` (UNBALANCED-PUT), `ForcewakeNotHeld` (FORCEWAKE-NOT-HELD),
`BadValue { what, value }` (BAD-VALUE). Markers print `why=<name>`.

A crate with its own richer refusal set (`i915_memory_core::MemError`, 26 variants) keeps its own enum with its own stable `name()`
and offers `to_i915()`, which today folds into `BadValue { what: <name>, value: 0 }`. Promoting such names to first-class
`I915Error` variants is an ABI bump (the enum is matched exhaustively by `name()`); until then `what` carries the name.

## 3. `WriteGuard` (`i915-common-core::guard`)

A `Mmio` that forwards READS and refuses every WRITE whose register is not on a `&'static [Reg]` allow-list. 8- and 16-bit writes
are always refused. `Mmio::w32` cannot return an error, so a refused write is not forwarded and is LATCHED: `first_refusal()`,
`refusals()`, `writes()`, `check() -> Result<(), I915Error>`. The caller checks the latch after each step and stops the device on a
refusal. *Exposing an operation is not granting it*: the allow-list is the in-code half of the grant; the loader's window grant is
the hardware half. **Planners never widen the allow-list.** A write-bearing plan (`BindPlan`, fence plan) stays unexecuted until an
owner-attended silicon increment extends the list.

## 4. `PlatformDescriptor` (`i915-devinfo-core`)

`lookup(devid: u16) -> Result<PlatformDescriptor, I915Error>`; unknown ids are `UnknownDevice`, never aliased to a neighbour.
`PlatformDescriptor { devid, sub: SubPlatform, info: DeviceInfo }` with `DeviceInfo { platform, graphics_ver, display_ver, gt,
engine_mask, ppgtt_type, ppgtt_size (log2), dma_mask_size, page_sizes, max_pat_index, has_llc, has_snoop, has_coherent_ggtt,
has_logical_ring_contexts, has_64bit_reloc, has_reset_engine, has_rc6, has_rps, has_runtime_pm, has_gt_uc }`.
Carried rows: SKL GT1-4, KBL GT1-3 (Gen9, the target) and CHV (Gen8, the negative control: recognised by name and refused by
`require_gen9()`). Every other generation-dependent fact a planner needs comes from this table, never from a constant typed for one
board (`ppgtt_size`, `dma_mask_size`, `has_llc`, ...). KBL: `dma_mask_size` 39, `ppgtt_size` 48, `ppgtt_type` Full.

## 5. `i915-wire` (permissive: MIT OR Apache-2.0, because the closed shim compiles against it)

A pointer-free little-endian frame: 40-byte header (`magic "I915"`, `major`, `minor`, `opcode`, `flags`, `request_id`,
`session_incarnation`, `payload_bytes`, `reserved`) followed by exactly `payload_bytes` bytes (`<= MAX_PAYLOAD` = 64 KiB). ABI
major 1; a frame of another major is `UnsupportedMajor` (reply status `BadVersion`); any minor of the same major is accepted.
`flags` bit 0 = REPLY, every other bit must be zero. A reply begins with a `u32` `Status` (`Ok`, `Unsupported`, `Malformed`,
`BadVersion`, `StaleSession`). `Opcode`: `Hello`, `GetReadout`, `SetMode`, `PageFlip`, `AllocScanout`, `SubmitBatch` (all but
`Hello` answered `Unsupported` until implemented). GPU data never travels in a frame: it lives in shared grants. Refusals are
`WireError` with names `WIRE-SHORT-BUFFER`, `WIRE-BAD-MAGIC`, `WIRE-UNSUPPORTED-MAJOR`, `WIRE-UNKNOWN-OPCODE`, `WIRE-UNKNOWN-FLAGS`,
`WIRE-RESERVED-NONZERO`, `WIRE-PAYLOAD-TOO-LARGE`, `WIRE-PAYLOAD-LENGTH-MISMATCH`. A stale `session_incarnation` is refused by the
device, not the codec.

## 6. The DMA contract (`coralos-hal`, MIT OR Apache-2.0)

**Principle: CPU address and device address are two different numbers and nothing derives one from the other.** A host may map a
region at any virtual address and place it at any bus address (behind an IOMMU, bus address != physical). The fake host
(`hal::fake::FakeDma`) maps `cpu = device + 0x7000_0000_0000` precisely so an identity assumption fails a test.

* The original surface (unchanged): `Dma::alloc_coherent(len, align, Coherence) -> Result<DmaRegion { virt, phys, len }, DmaRefusal>`.
  No `free`: a grant lives in the task's DMA window and goes back with the task, so a driver allocates once at bring-up and reuses.
* The GPU-grade surface (additive, a provided method, so existing hosts and iwlwifi are untouched):

  ```text
  Dma::alloc(DmaRequest { size, align, addr_width, cache, budget }) -> Result<DmaGrant { cpu_mapping, device_addr, len }, DmaRefusal>
  ```
  * `size` bytes (rounded up to whole pages), zeroed and physically contiguous; `align` a power of two <= 4096.
  * `addr_width` (12..=64): the device's DMA address width in bits; the WHOLE grant must lie below `1 << addr_width`. KBL:
    `PlatformDescriptor.info.dma_mask_size` = 39. A grant that does not fit is `BeyondAddrWidth { device_addr, len, width }`; a width
    outside 12..=64 is `BadAddrWidth`.
  * `cache`: `Coherence::Snooped` (WB mapping, the device snoops) or `NonSnooped` (WC mapping + store fence). Recommended for
    GPU page-table pages and ring/context memory: `NonSnooped`, so a CPU store is never stranded in a line the page walker cannot
    see. A host that cannot map WC refuses `CacheTypeUnavailable`; it never answers with a write-back page.
  * `budget`: the most bytes (in whole pages) this ONE request may take; more is `OverQuota { len }` before the host is asked.
    The per-task window quota is the host's, independent of this.
  * The default method applies the shared request rules (`check_dma_request`), calls `alloc_coherent`, then checks the grant against
    `addr_width` (`check_dma_width`). A host that can place below a width overrides it so the check happens BEFORE memory is taken
    (`FakeDma` does; the default cannot un-grant, which is why the post-check refusal is named and the grant is never used).
  * Everything a planner emits names `device_addr` only. The executor keeps `cpu_mapping` next to the `device_addr` it was granted
    and does the CPU-side stores itself.
* Refusals (`DmaRefusal`), all carrying their values: `ZeroLength`, `BadAlign`, `Misplaced`, `OverQuota`, `OutOfMemory`,
  `CacheTypeUnavailable`, `BadAddrWidth`, `BeyondAddrWidth`, `Host(word)`.
* Tests: `hal/src/tests.rs` (`alloc_*`, `width_check_*`) and `crates/i915-memory-core/tests/dma_contract.rs` (table pages through
  `Dma::alloc`, CPU != device).

## 7. `BindPlan` and the memory planners (`i915-memory-core`)

Pure planners over one address space: `Gtt<V, T>` is either the GGTT (`Gtt::new_ggtt`) or a full 48-bit PPGTT (`Gtt::new_ppgtt`).
Gen9 only; other generations are refused by name (`UNSUPPORTED-GENERATION`, `NOT-FULL-PPGTT`).

```text
Gtt::plan_bind(&mut self, src: &mut impl TablePageSource, &BindRequest) -> Result<BindPlan, MemError>
Gtt::plan_unbind(&mut self, VmaId) -> Result<UnbindPlan, MemError>
Gtt::check_access(va, len) -> Result<VmaId, MemError>     // the device-side denial: stale / guard / beyond-48-bit
Gtt::walk(va) -> Result<Walk, MemError>                    // PML4/PDP/PD/PT indices and the table pages that exist
```

* `BindRequest { pages: &[u64] (device addresses), size, alignment, guard, cache: CacheLevel, flags, place: Placement }`.
  `Placement { mode: Best | Low | High | Fixed(off), bias, mappable, zone_4g, evict }` (Linux `PIN_*`).
* **Table pages are requested, never allocated**: `TablePageSource::table_page(TablePageRequest { level, base_va }) ->
  TablePage { device_addr }`. The executor implements it over `Dma::alloc` (see 6). A page that aliases the PML4 or a scratch
  page, is unaligned, or exceeds the DMA mask is `TABLE-PAGE-REFUSED`. Pages obtained by a plan that then fails are parked in a
  spare pool (cap `MAX_SPARE` = 64) and used first by the next plan; overflow is counted in `leaked_pages()`, never dropped silently.
* `BindPlan` carries an ordered `ops() -> &[Op]` (cap `MAX_OPS` = 512 per plan; `PLAN-TOO-LARGE` otherwise - split the bind):

  | `Op` | meaning |
  |---|---|
  | `AwaitRetire { vma }` | wait until the GPU no longer uses `vma` (precedes every unmap of a live VMA) |
  | `FillTable { table, entry }` | fill a whole 4 KiB table page with one entry |
  | `TableEntry { table, index, value }` | link / unlink one directory entry |
  | `PteRun { table, index, count, page_offset }` | leaf PTEs for `pages[page_offset..]` (encoded with the request's cache level and flags) |
  | `PteFill { table, index, count, value }` | leaf PTEs all `value` (scratch) |
  | `GgttRun { first, count, page_offset }` / `GgttFill { first, count, value }` | GGTT entries (BAR0 + 8 MiB + 8 * entry) |

  `writes()` expands lazily to `Write { target: Target::Ggtt { mmio_offset } | Target::Table { device_addr, byte_offset }, value }`;
  `flushes()` yields `Flush::CpuCache { device_addr, byte_offset, bytes }` per touched table span, then one TLB step:
  `GgttInvalidate(RegWrite { GFX_FLSH_CNTL_GEN6 = 0x101008, 1 })` for the GGTT, `PpgttTlb` (an engine-level flush owned by
  `i915-gt-core`) for a PPGTT. `evicted()` lists victims (cap `MAX_EVICT` = 16); `offset()` is the address the GPU uses (past the
  leading guard); `new_tables()` counts table pages consumed.
* Ordering the executor may rely on: a table page is filled before the entry that links it; a victim's `AwaitRetire` precedes its
  scratch fill; every victim is unmapped before the new range is written; every store precedes every flush.
* Guarantees the model keeps: guard pages and the scratch tail are written as scratch on every bind; an unbound or reused range is
  `STALE-MAPPING`; a guard page is `GUARD-PAGE-ACCESS`; an address at or above bit 48 is `VA-BEYOND-48BIT` (a range crossing it is
  refused at the first offending address, `1 << 48`); ids are never reused; pinned nodes and the GGTT error-capture reservation are
  never victims and cannot be unbound.
* Bounds: VMAs per space `V` (`VMA-TABLE-FULL`), table pages per space `T` (`TABLE-TABLE-FULL`), ops, victims, spare pool.
* Deliberately NOT ported (stated where they occur): empty table pages are kept after unbind (hal `Dma` has no free); Linux's random
  replacement slot (`random_offset`) is replaced by deterministic LRU; 64K/2M huge pages, local memory, vGPU ballooning, GuC GGTT
  regions, the blitter-bind GGTT path, Gen6/7/12/MTL encoders.
* Fence registers: `fence::plan_fence(id, start, size, tiling, stride) -> FencePlan` (five steps: LO<-0, read LO, HI, LO, read LO).

### Marker (P2)

`[I915-MEMORY va_bits=48 stale_access=denied msi=ok]` - `va_bits=48` from `PlatformDescriptor.info.ppgtt_size`, `stale_access=denied`
from a `check_access` probe after an unbind, `msi` from the IRQ qualification (not this crate).

## 8. Sessions, objects, contexts, execbuffer (`i915-gem-core`)

`Gem<V, T>` is the whole GEM state of one device as a planner. `V`/`T` are the VMA and page-table capacities of each address space
(`Gtt<V, T>`; those capacities are `i915-memory-core`'s). Every table of this crate is an `alloc` collection (doctrine
RUNTIME-TABLES-HAVE-NO-CAP, DECISIONS.md D9), so a `Gem` is a handful of empty vectors until used.

* **Session** (`drm_file`): `open_session(caps, PpgttInit, sink) -> SessionId`. A `SessionId` is an incarnation (u64 counter, never
  reused); a closed session's id is `SESSION-GONE` forever. Opening makes context 0 on a new address space from the executor's pages.
* **Handles are session-scoped and generation-tagged**: `gen << 16 | (slot + 1)`, never 0. The lookup reads ONLY the caller's own table.
  A live handle of another session is `CROSS-SESSION-HANDLE` (errno ENOENT; EINVAL from `GEM_CLOSE`, as `drm_gem_handle_delete`); an
  earlier occupant of a slot is `STALE-HANDLE`; anything else `HANDLE-NOT-IN-SESSION`. A slot whose generation would wrap is retired.
  An object is freed (unbound everywhere, pages released, `Effect::ObjectFreed`) only when it has no handle, no unretired request and
  no mmap lease: closing a handle never frees memory the GPU may still write.
* **Backing pages are requested, never allocated**: `PageSource::alloc_pages(PageRequest{obj, bytes, cache})` then `pages(obj)` (device
  addresses). Pages are lazy (first `set_domain`, mmap lease or execbuffer). `Effect::FreePages` releases them.
* **Everything the world must do is an `Effect`** or a bind/unbind plan streamed to the `EffectSink`. Order of one execbuffer:
  binds (eviction victims inside the plan, `AwaitRetire` ahead of their scratch fill) -> `PatchReloc` -> `Clflush` ->
  `AwaitRequest`/`AwaitInFence`/`AwaitSyncobj` -> `SignalSyncobj`/`OutFence` -> `Submit` (the ELSP plan).
* **Execbuffer2** = `i915-uapi-core::validate_execbuf2` (all pre-execution refusals, over this crate's `ExecEnv` view) + reserve
  (`eb_reserve` passes: 0 no eviction, 1 LRU eviction, 2 unpin + evict the whole VM; `ENOSPC` after) + relocation as `PatchReloc`
  (the MINIMAL FAITHFUL path: no GPU relocation, no slow path) + `EngineSet::build_request` + `dequeue`. A submission's own VMAs are
  pinned for its duration and never victims. Softpinned objects must land exactly where asked or the submission fails
  (`EXEC-PINNED-MISPLACED`).
* **Contexts**: params per `ctx_setparam`/`set_proto_ctx_param`; VM and ENGINES only at creation; SSEU/PXP/low-latency refused by name.
  The per-engine hardware context (`i915_submit_core::Engine::create_context`) is created by the executor and attached
  (`attach_engine_context`); a submission without one is `EXEC-ENGINE-CONTEXT-MISSING`.
* **Activity**: a request is COMPLETE when `seqno_passed(hw_seqno(engine, ectx), seqno)` (`observe_seqno` feeds the HWSP values) and
  RETIRED by `retire()`, which also frees what that unblocks. `busy()` and `wait_plan()` are Linux's; `wait_finish` is the ioctl's
  `timeout_ns` accounting (the caller blocks, this crate never does).
* **Hangs**: `request_reset(engine, ectx, seqno, guilty, now_ms)`. Linux bans a recoverable, bannable context on the THIRD hang inside
  120 s (two-slot history), a non-recoverable one on its first; the client's ban score (+3 per ban, +1 per hang within 60 s) refuses new
  contexts from 9. PORT ADDITION: `MAX_GUILTY_BEFORE_BAN` (16) bans a slow hanger Linux never would.
* **mmap**: `mmap_offset` (policy: GTT needs the aperture, WC/UC need PAT, WB always, FIXED never), `mmap_open` (a lease, only for the
  session the offset was granted to), `mmap_close`. Closing a handle revokes NEW mappings, not leases.
* **Reclaim**: `shrink(target_pages)` purges idle `MADV_DONTNEED` objects (unbind, free pages, `__I915_MADV_PURGED`); a purged object
  answers `retained = false` and `OBJECT-PURGED` (`-EFAULT`) on use.
* **Refused by name**: `GEM_USERPTR` (`USERPTR-REFUSED`: EINVAL for the zero-size probe, ENODEV otherwise), local memory / non-SYSTEM
  placements, protected content, SET_PAT, SSEU reconfiguration, parallel/bonded submission, the command parser.
* **No caps on runtime populations**: sessions, handles, objects, contexts, address spaces, requests, mmap offsets, leases and seqno
  observations grow with memory. Growth is fallible; exhaustion (a failed `try_reserve`, or the `set_table_budget` quota) is
  `GEM-NOMEM` (ENOMEM) naming the table, raised BEFORE the thing exists: a refused create leaves no object, handle, context or VM.
  Reusing a freed slot never charges. Fixed by hardware/ABI (`limits.rs`): `MAX_ENGINES` 5, `MAX_USER_ENGINES` 64, the handle index
  width (`HANDLE_IDX_BITS` 20 -> `HANDLE-SPACE-EXHAUSTED` at 2^20 - 1 live handles in one session), `HANG_HISTORY` 2. Policy
  threshold (not a table): `MAX_GUILTY_BEFORE_BAN` 16. Object size <= system RAM.

### Markers (P5)

`[I915-GEM REFUSED why=<GemError::name()> errno=<n>]` per refused ioctl (the names are unique, uppercase, dash-separated;
`tests/bounds.rs::errors_have_distinct_stable_names`).

## Additive changes since CONTRACT_ABI 1 was cut

* 2026-10-12: `coralos-hal` gains `Dma::alloc` (provided), `DmaRequest`, `DmaGrant`, `check_dma_request`, `check_dma_width`, and
  `DmaRefusal::{BadAddrWidth, BeyondAddrWidth}`. A downstream `match` on `DmaRefusal` that was exhaustive must add them (only `rt`
  constructs the enum; nothing in this repo matches it).
