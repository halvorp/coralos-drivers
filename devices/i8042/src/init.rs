// SPDX-License-Identifier: GPL-2.0-only
//! Controller and keyboard bring-up, then the decoder the rest of the device runs on.
use crate::log::{self, Log};
use crate::pipeline::Pipeline;
use atkbd_scancode_core::decode::{Config, Decoder};
use i8042_core::bringup::{bring_up, Failed, Options, Step, Verdict};
use i8042_core::controller::Controller;
use i8042_core::ps2::Ps2Dev;
use i8042_core::regs::CTR_XLATE;
use i8042_core::Port;

/// A device that owns the controller: the controller, and the pipeline its bytes go through.
pub struct Device<P: Port> {
    pub ctl: Controller<P>,
    pub pipe: Pipeline,
    /// True when a keyboard command step refused and the device runs on the firmware's keyboard state.
    pub degraded: bool,
}

/// Is this step one of the keyboard's own commands (after the controller is configured and the port enabled)?
/// A refusal here leaves a working controller; an EC-emulated 8042 that ignores a LED or typematic command still
/// delivers scancodes, so the device carries on and says so, rather than leaving the machine without a keyboard.
fn is_keyboard_step(s: Step) -> bool {
    matches!(s, Step::KbdReset | Step::GetId | Step::ScanSet | Step::Leds | Step::Typematic | Step::ScanEnable)
}

/// Bring the controller and keyboard up, naming every step on `log`. Err only when the CONTROLLER refused
/// (no controller, a stuck wait, an unstable CTR, the port would not enable).
pub fn init<P: Port, L: Log>(io: P, opts: Options, log: &mut L) -> Result<Device<P>, Failed> {
    let mut ctl = Controller::new(io);
    let mut dev = Ps2Dev::new();
    // Bytes the keyboard sends while a command is in flight (a key pressed during bring-up) are not decoded: the
    // decoder does not exist until the scan set is known.
    let mut lost = 0u32;
    let r = {
        let mut report = |s: Step, v: Verdict| {
            if let Verdict::Failed { .. } = v {
                // The FAIL marker is printed once, below, with the verdict of what happens next.
                return;
            }
            log::step(log, s, v);
        };
        bring_up(&mut ctl, &mut dev, opts, &mut report, &mut |_| lost += 1)
    };
    let (set, translated, degraded) = match r {
        Ok(b) => {
            log::emit(
                log,
                format_args!(
                    "[I8042-DEV OK set={} translated={} id={:#06x} keylock={} flushed={} typed-during-init={}]",
                    b.set, b.ctr.translated as u8, b.probe.id, b.ctr.keylock as u8, b.flushed, lost
                ),
            );
            (b.set, b.ctr.translated, false)
        }
        Err(f) if is_keyboard_step(f.step) => {
            log::emit(log, format_args!("[I8042-DEV FAIL step={} why={}]", f.step.name(), f.why.name()));
            log::emit(log, format_args!("[I8042-DEV DEGRADED step={} - running on the keyboard's own state, set 2]", f.step.name()));
            (2, ctl.ctr & CTR_XLATE != 0, true)
        }
        Err(f) => {
            log::emit(log, format_args!("[I8042-DEV FAIL step={} why={}]", f.step.name(), f.why.name()));
            return Err(f);
        }
    };
    let dec = Decoder::new(Config { set, translated, scroll: false }).expect("set 2 is a supported scancode set");
    Ok(Device { ctl, pipe: Pipeline::new(dec), degraded })
}
