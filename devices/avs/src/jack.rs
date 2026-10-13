// SPDX-License-Identifier: GPL-2.0-only
//! The jack interrupt machine. The DA7219's accessory detect raises its interrupt line (GPP_F10, LEVEL LOW) and holds it until the events
//! are read and cleared over I2C; the pad interrupt owner delivers that as a jack event and masks the pad until it is told the line was
//! serviced ([`Machine::rearm_jack`]). One pass is `da7219_aad_irq_thread`: read both event registers, read the status, clear the events,
//! decode, and run the follow-up plan (drive the output, enable button detection, un-drive on removal). The decode says what the jack is.
//!
//! LEVEL-LOW means a pass must leave the line HIGH: after each pass the pad is read back; if it is still asserted another pass runs, at
//! most [`MAX_PASSES`] of them - a line that will not deassert is a fault ("jack-irq-storm"), not a loop. If the pad cannot be read, passes
//! continue until the codec reports no events (`IRQ_NONE`).
//!
//! Idle is a blocked wait on the jack event: nothing here polls. The one timer is the codec's delayed ground-switch enable
//! (`da7219_aad_jack_det_work`), armed on an insertion and cancelled by detect-complete or removal; the loop's wait carries it as a deadline.
use crate::device::Device;
use crate::gpio::jack_line_asserted;
use crate::machine::Machine;
use da7219_core::aad::{SND_JACK_BTN_0, SND_JACK_BTN_1, SND_JACK_BTN_2, SND_JACK_BTN_3};

/// Passes of the accessory-detect handler before the line is declared stuck.
pub const MAX_PASSES: u32 = 8;

const BUTTONS: [u16; 4] = [SND_JACK_BTN_0, SND_JACK_BTN_1, SND_JACK_BTN_2, SND_JACK_BTN_3];

impl<M: Machine> Device<M> {
    /// Service the codec's interrupt until its line is quiet, then re-arm the pad.
    pub fn service_jack_pass(&mut self) {
        let Some(board) = self.board else { return };
        let mut quiet = false;
        for _ in 0..MAX_PASSES {
            self.stats.jack_passes += 1;
            let Some(codec) = self.codec.as_mut() else { return };
            let o = match codec.jack_irq(&mut self.rig) {
                Ok(o) => o,
                Err(e) => {
                    self.fault(e.step(), e.detail());
                    return;
                }
            };
            if o.spurious {
                quiet = true;
                break;
            }
            // delayed ground switch: armed by an insertion, cancelled by detect-complete or removal
            let now = self.rig.now();
            if o.pole.is_some() || !o.state_jack_inserted {
                self.gnd_deadline = None;
            } else if let Some(ms) = o.ground_switch_after_ms {
                self.gnd_deadline = Some(now + ms as u64);
            }
            // buttons: a report whose mask names a button restates it
            let mut buttons = self.buttons;
            for r in [o.press_report, o.report].into_iter().flatten() {
                for (i, bit) in BUTTONS.iter().enumerate() {
                    if r.mask & bit != 0 {
                        if r.report & bit != 0 {
                            buttons |= 1 << i;
                        } else {
                            buttons &= !(1 << i);
                        }
                    }
                }
            }
            let new = o.report.map(|r| self.classify(&r)).unwrap_or(self.jack);
            let buttons_changed = buttons != self.buttons;
            self.buttons = if new == avs_wire::Jack::Absent { 0 } else { buttons };
            if new != self.jack {
                self.jack_changed(new);
            } else if buttons_changed {
                self.send_jack_event();
            }
            if let Some(false) = jack_line_asserted(&mut self.rig, board.jack_pad) {
                quiet = true;
                break;
            }
        }
        if !quiet {
            // a line that will not go quiet is NOT re-armed: the pad owner keeps it masked, so a stuck codec costs MAX_PASSES passes and
            // not a core. A client's next START tries it again.
            self.jack_masked = true;
            self.fault("jack-irq-storm", MAX_PASSES);
            return;
        }
        self.rig.m.rearm_jack();
        // The accessory detect drives the jack's output stage on when it finds headphones (the follow-up plan of a detect-complete). With
        // no stream running that would leave the codec out of the SAFE STATE the status promises, so it is restored and re-proven; the
        // next power-up drives the output again ([`crate::codec::Codec::power_up`]).
        if self.dsp.is_none() {
            if let Some(c) = self.codec.as_mut() {
                // a pass that did not drive the output (a button, a spurious line) leaves the proven state alone
                if c.is_safe(&mut self.rig) {
                    self.st.safe_verified = true;
                } else {
                    match c.safe_state(&mut self.rig) {
                        Ok(()) => self.st.safe_verified = true,
                        Err(e) => self.fault(e.step(), e.detail()),
                    }
                }
            }
        }
    }
}
