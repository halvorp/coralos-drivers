# VCS0 on i915.device: the per-generation audit (Cherryview Gen8 + Kaby Lake Gen9.5)

What Linux does to run the VIDEO engine (VCS0, `GEN6_BSD_RING_BASE` 0x12000) on the two parts CoralOS targets,
read from the pinned linux-ref (`references/linux-ref`, v7.0-rc6, `drivers/gpu/drm/i915/`), against what this repo
already carries. Every row is a citation, not a recollection. CHV is the Z83 (8086:22B0): it is the silicon witness
for the generation-independent engine code. KBL GT2 is the Pixelbook Go (8086:591C), step B0 on silicon
(`[I915-KBL-DEV IDENTIFIED ... step=B0]`).

| # | Concern | Linux (file:line) | Gen8 CHV | Gen9 KBL | In this repo today |
|---|---|---|---|---|---|
| 1 | Engine context image size | `intel_engine_context_size` gt/intel_engine_cs.c:273-341; sizes :46-50 | VIDEO = `GEN8_LR_CONTEXT_OTHER_SIZE` (2 pages) | same, 2 pages (render is 22) | `i915-gt-core` engine.rs: the constants are there, but `context_size` is Gen9-only (`graphics_ver != 9` → UnsupportedGeneration, :121) |
| 2 | LRC register-state layout | `reg_offsets` gt/intel_lrc.c:643-677 | `gen8_xcs_offsets` | `gen9_xcs_offsets` | `i915-submit-core` lrc_offsets.rs carries gen8 + gen9 xcs/rcs tables, tested against Linux's `set_offsets` |
| 3 | MI_MODE slot in the image | `lrc_ring_mi_mode` intel_lrc.c:679-690 | **none** for xcs (`-1`: render 0x58 only) | 0x54 | `lrc_offsets::lrc_ring_mi_mode(gen, render)` |
| 4 | PPGTT in the image | `init_ppgtt_regs` intel_lrc.c:889-903 | CHV is 3-level/32-bit: `ASSIGN_CTX_PDP` 3..0 | 4-level/48-bit: `ASSIGN_CTX_PML4` | `i915-submit-core` lrc_state.rs:140-143: 4-level only (PDP0 = the PML4). The CHV 3-level arm (PDP3..0) is **not ported** |
| 5 | Forcewake for VCS0 | `intel_uncore_fw_domains_init` intel_uncore.c:2210-2225; ranges `__chv_fw_ranges` / `__gen9_fw_ranges` | `FORCEWAKE_MEDIA_VLV` / `ACK_MEDIA_VLV` (VLV_DOMAINS) | `FORCEWAKE_MEDIA_GEN9` / `ACK_MEDIA_GEN9` | domain.rs has both tables. VLV is a negative control only; `__chv_fw_ranges` is not ported; the Gen9 media domain is live |
| 6 | Per-engine MOCS | `get_mocs_settings` gt/intel_mocs.c:454-534; `init_mocs_table` per engine | **no MOCS** (no table; flags 0) | `skl_mocs_table`, engine block `__GEN9_VCS0_MOCS0` 0xc900 | `i915-gt-core` mocs.rs Gen9 tables; i915.device programs RCS0's block only (exec/allow.rs P3_MOCS_REGS) |
| 7 | Engine workarounds | `xcs_engine_wa_init` gt/intel_workarounds.c:2686-2700 | none | `WaKBLVECSSemaphoreWaitPoll:kbl` for `IS_GRAPHICS_STEP(A0, F0)`: `RING_SEMA_WAIT_POLL(base) = 1`. Atlas is B0 → **applies to VCS0** | not ported (wa_tables.rs has the GT/RCS rows) |
| 8 | Request flush on VCS | `gen8_emit_flush_xcs` gt/gen8_engine_cs.c:~85-110 | `MI_FLUSH_DW` + `MI_INVALIDATE_TLB` + `MI_INVALIDATE_BSD` on invalidate | same | `i915-gt-core` emit.rs:174 `gen8_emit_flush_xcs` is ported |
| 9 | GGTT size | `chv_get_total_gtt_size` gt/intel_ggtt.c:1169-1178 (`1 << (20 + GGMS)`) | needed | Gen8+ `gen8_get_total_gtt_size` | `i915-memory-core` probe.rs:8: CHV **not ported** |
| 10 | GT power on the media well | CHV: `VLV_GTLC_PW_STATUS` wells (vlv_suspend.c:284, :376-380) under RC6 | the media well must be awake before VCS0 MMIO | Gen9: forcewake MEDIA covers it | coral-i915-gem `gt_power.rs` (in-tree, silicon-proven on the Z83) is the CoralOS reference |
| 11 | Engine IRQ bit | `GEN8_GT_VCS0_IRQ` i915_reg.h:875 (bank 1, shift 0) | same on Gen8 and Gen9 | same | `i915-gt-regs-core` i915_reg.rs:31-36 |
| 12 | Engine reset | `intel_reset.c` `gen8_reset_engines` / `RING_RESET_CTL` handshake | same mechanism | same | i915.device's reset planner writes `RING_RESET_CTL` of every engine already (allow.rs) |
| 13 | AVC decode program | intel-vaapi-driver `gen9_dec_hw_context_init` → `gen8_dec_hw_context_init` (gen9_mfd.c:2019) | gen8 MFD | **the same gen8 MFD layouts** | coralos `coral-i915-media` (silicon-proven on CHV via coral-drm); only the per-gen surface MOCS and address width differ |

## What this means for the ladder (VCS-PLAN.md)
- These rows are table data and are generation-independent: rows 2, 3, 8, 11, 12 are already carried for both generations,
  and the engine layer can run VCS0 on CHV and on KBL from these tables.
- CHV-only work, for the Z83 witness:
  - rows 1 (let Gen8 through, with its own size arm);
  - row 4 (3-level PDP);
  - row 5 (live VLV domains + `__chv_fw_ranges`);
  - row 6 ("no MOCS" as an explicit row);
  - row 9 (GGTT size);
  - row 10 (media well).
- KBL-only work, which the Z83 cannot prove:
  - row 6 (the VCS0 MOCS block);
  - row 7 (`WaKBLVECSSemaphoreWaitPoll`, B0 is in range);
  - row 4's 4-level arm on VCS;
  - the default context image recorded on silicon.
