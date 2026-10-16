// SPDX-License-Identifier: GPL-2.0-only
//! The display AUTHORITY's calls (D18, i915-wire 1.5): `DISPLAY-SET-MODE` and the scanout pair. Every one of them is refused
//! `-EACCES` (`DISPLAY-NOT-AUTHORIZED`) unless the calling session is the one [`UapiService::bind_display_authority`] bound - and that
//! binding is made by the transport on a capability, never on anything a request says.
use super::fail::Fail;
use super::service::{Done, Hw, UapiService};
use crate::display::answer::NOT_AUTHORIZED;
use crate::exec::dma::CpuMem;
use crate::log::{emit, Log};
use coralos_hal::{Dma, Mmio};
use i915_display_modeset_core::initial::OfferedMode;
use i915_wire::msg::display::DisplayModeReq;
use i915_wire::msg::{self, Body};
use i915_wire::{errno, Header, Opcode, Status};

impl UapiService {
    pub(crate) fn call_display<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, inc: u64, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        if self.authority != Some(inc) {
            return Err(Fail::new(errno::EACCES, NOT_AUTHORIZED));
        }
        match h.opcode {
            Opcode::DisplaySetMode => {
                let r = DisplayModeReq::from_payload(p)?;
                let a = self.display.adopted().ok_or(Fail::new(errno::ENODEV, "DISPLAY-NOT-ADOPTED"))?;
                let asked = OfferedMode { width: r.width, height: r.height, refresh_millihz: r.refresh_millihz };
                if let Err(e) = a.check_mode(asked) {
                    emit(hw.log, format_args!("[I915-DISPLAY mode refused name={} asked={}x{}@{} adopted={}x{}@{}]", e.name(), asked.width, asked.height, asked.refresh_millihz, e.adopted.width, e.adopted.height, e.adopted.refresh_millihz));
                    return Err(Fail::new(errno::EINVAL, e.name()));
                }
                Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?))
            }
            // 2026-10-16 (D18): the presentation increment's service glue - the pair's GEM backing and GGTT pin, the PLANE_SURF write,
            // SURFLIVE polling, the parked wait - is not in this device yet, so these calls are refused by name (DELETE-WHEN: devices/i915/Cargo.toml gains the `p4-present` feature that compiles that glue in)
            _ => Err(Fail::new(errno::EOPNOTSUPP, "SCANOUT-INCREMENT-NOT-ENABLED")),
        }
    }
}
