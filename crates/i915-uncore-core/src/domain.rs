// SPDX-License-Identifier: GPL-2.0-only
//! The forcewake domains and their set / ack registers.
use i915_common_core::Reg;
use i915_gt_regs_core as r;

/// `enum forcewake_domain_id` (intel_uncore.h:49-65), the three Gen9 BC uses: RENDER = 0, GT = 1, MEDIA = 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainId {
    Render = 0, // intel_uncore.h:50
    Gt = 1,     // intel_uncore.h:51
    Media = 2,  // intel_uncore.h:52
}

/// `enum forcewake_domains` bits = `BIT(domain id)` (intel_uncore.h:70-73; the BUILD_BUG_ONs at intel_uncore.c:2089-2091).
pub const FORCEWAKE_RENDER: u32 = 1 << 0; // intel_uncore.h:71
pub const FORCEWAKE_GT: u32 = 1 << 1; // intel_uncore.h:72
pub const FORCEWAKE_MEDIA: u32 = 1 << 2; // intel_uncore.h:73
/// All three Gen9 BC domains (`uncore->fw_domains` after `intel_uncore_fw_domains_init`, intel_uncore.c:2210-2219).
pub const FORCEWAKE_ALL: u32 = FORCEWAKE_RENDER | FORCEWAKE_GT | FORCEWAKE_MEDIA;

/// One `struct intel_uncore_forcewake_domain`'s static part: which bit, which set register, which ack register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Domain {
    pub id: DomainId,
    /// `d->mask = BIT(domain_id)`.
    pub mask: u32,
    /// `d->reg_set` (written `_MASKED_BIT_ENABLE/DISABLE(FORCEWAKE_KERNEL)`).
    pub set: Reg,
    /// `d->reg_ack`.
    pub ack: Reg,
    pub name: &'static str,
}

/// The Gen9 domains, in `intel_uncore_fw_domains_init` order (intel_uncore.c:2210-2219): RENDER, GT, MEDIA.
pub static GEN9_DOMAINS: [Domain; 3] = [
    Domain { id: DomainId::Render, mask: FORCEWAKE_RENDER, set: r::FORCEWAKE_RENDER_GEN9, ack: r::FORCEWAKE_ACK_RENDER_GEN9, name: "RENDER" }, // intel_uncore.c:2212-2214
    Domain { id: DomainId::Gt, mask: FORCEWAKE_GT, set: r::FORCEWAKE_GT_GEN9, ack: r::FORCEWAKE_ACK_GT_GEN9, name: "GT" },                      // intel_uncore.c:2215-2217
    Domain { id: DomainId::Media, mask: FORCEWAKE_MEDIA, set: r::FORCEWAKE_MEDIA_GEN9, ack: r::FORCEWAKE_ACK_MEDIA_GEN9, name: "MEDIA" },       // intel_uncore.c:2218-2219
];

/// Valleyview / Cherryview domains (intel_uncore.c:2220-2225): the NEGATIVE CONTROL. A Gen9 part must never be driven with
/// these; tests prove they time out against a Gen9 register script. Not reachable through [`crate::Uncore::gen9`].
pub static VLV_DOMAINS: [Domain; 2] = [
    Domain { id: DomainId::Render, mask: FORCEWAKE_RENDER, set: r::FORCEWAKE_VLV, ack: r::FORCEWAKE_ACK_VLV, name: "RENDER" },       // intel_uncore.c:2222-2223
    Domain { id: DomainId::Media, mask: FORCEWAKE_MEDIA, set: r::FORCEWAKE_MEDIA_VLV, ack: r::FORCEWAKE_ACK_MEDIA_VLV, name: "MEDIA" }, // intel_uncore.c:2224-2225
];

/// The ONLY registers the Gen9 forcewake protocol writes: the three SET registers. This is the device's MMIO write allow-list for
/// increment 1 (the ack registers are read-only).
pub const WRITE_ALLOWLIST: [Reg; 3] = [r::FORCEWAKE_GT_GEN9, r::FORCEWAKE_MEDIA_GEN9, r::FORCEWAKE_RENDER_GEN9];
