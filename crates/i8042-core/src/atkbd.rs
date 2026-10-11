// SPDX-License-Identifier: GPL-2.0-only
//! The AT keyboard's COMMANDS — Linux `drivers/input/keyboard/atkbd.c` (`atkbd_probe`, `atkbd_select_set`,
//! `atkbd_set_repeat_rate`, `atkbd_set_leds`, `atkbd_activate`, `atkbd_deactivate`), each a
//! [`Ps2Dev::command`]. Decoding the keyboard's bytes is `atkbd-scancode-core`'s.
//! Copyright (c) 1999-2002 Vojtech Pavlik.

use crate::controller::Controller;
use crate::ps2::{is_keyboard_id, Ps2Dev, Ps2Error};
use crate::Port;

pub const CMD_SETLEDS: u16 = 0x10ed; // atkbd.c:136
pub const CMD_GSCANSET: u16 = 0x11f0; // atkbd.c:137
pub const CMD_SSCANSET: u16 = 0x10f0; // atkbd.c:138
pub const CMD_GETID: u16 = 0x02f2; // atkbd.c:139
pub const CMD_SETREP: u16 = 0x10f3; // atkbd.c:140
pub const CMD_ENABLE: u16 = 0x00f4; // atkbd.c:141
pub const CMD_RESET_DIS: u16 = 0x00f5; // atkbd.c:142
pub const CMD_RESET_BAT: u16 = 0x02ff; // atkbd.c:146
pub const CMD_EX_ENABLE: u16 = 0x10ea; // atkbd.c:148

/// GETID's buffer before the command: "initialize with invalid values".
pub const ID_INVALID: u8 = 0xa5; // atkbd.c:837
/// The id of a keyboard that answers SETLEDS but not GETID.
pub const ID_LEDS_ONLY: u16 = 0xabba; // atkbd.c:848
/// The NCD terminal keyboard, refused on a translating controller.
pub const ID_NCD: u16 = 0xaca1; // atkbd.c:857
/// The extended-set enable parameter (`atkbd_select_set` with `allow_extra`).
pub const EX_ENABLE_PARAM: u8 = 0x71; // atkbd.c:903

/// The typematic period table, ms.
pub const REPEAT_PERIODS_MS: [u16; 32] = [
    33, 37, 42, 46, 50, 54, 58, 63, 67, 75, 83, 92, 100, 109, 116, 125,
    133, 149, 167, 182, 200, 217, 232, 250, 270, 303, 333, 370, 400, 435, 470, 500,
]; // atkbd.c:585-587
/// The typematic delay table, ms.
pub const REPEAT_DELAYS_MS: [u16; 4] = [250, 500, 750, 1000]; // atkbd.c:588-589
/// The input core's repeat defaults when the driver sets none.
pub const INPUT_DEFAULT_DELAY_MS: u16 = 250; // input.c:2352
pub const INPUT_DEFAULT_PERIOD_MS: u16 = 33; // input.c:2352

/// LED bits of SETLEDS' parameter.
pub const LED_SCROLL: u8 = 1; // atkbd.c:612
pub const LED_NUM: u8 = 2; // atkbd.c:613
pub const LED_CAPS: u8 = 4; // atkbd.c:614

/// A refusal, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KbdFail {
    /// A command failed underneath.
    Command(Ps2Error),
    /// GETID answered, and not as a keyboard (atkbd.c:852-853).
    NotAKeyboard { id0: u8 },
    /// An NCD terminal keyboard behind a translating controller (atkbd.c:857-862).
    NcdOnTranslatingController,
    /// Set 3 negotiation (`target_set == 3`) is not ported: the decoder this feeds speaks set 2.
    Set3NotPorted,
}

impl KbdFail {
    pub const fn name(self) -> &'static str {
        match self {
            KbdFail::Command(e) => e.name(),
            KbdFail::NotAKeyboard { .. } => "NOT-A-KEYBOARD",
            KbdFail::NcdOnTranslatingController => "NCD-ON-TRANSLATING-CONTROLLER",
            KbdFail::Set3NotPorted => "SET3-NOT-PORTED",
        }
    }
}

/// What `atkbd_probe` established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    pub id: u16,
    /// The reset's outcome when asked for: `Ok(bat)` carries the first reply byte (0xaa = the keyboard's
    /// self-test passed; Linux checks only that a byte came, so this port names the byte and lets the host
    /// judge it). `Err` is a reset that failed: Linux warns and carries on (atkbd.c:819-823).
    pub reset: Option<Result<u8, Ps2Error>>,
    /// RESET_DIS failed: Linux logs "Failed to deactivate keyboard" and carries on (atkbd.c:760-763).
    pub deactivate: Result<(), Ps2Error>,
}

/// `atkbd_probe` (atkbd.c:808-873) with `atkbd_skip_getid` and `atkbd_skip_deactivate` false. `reset` is
/// the `atkbd.reset` module parameter — FALSE by default on x86 (atkbd.c:40-44).
pub fn probe<P: Port>(
    dev: &mut Ps2Dev,
    c: &mut Controller<P>,
    reset: bool,
    translated: bool,
    rx: &mut dyn FnMut(u8),
) -> Result<Probe, KbdFail> {
    let reset_outcome = if reset {
        let mut p = [0u8; 2];
        Some(dev.command(c, &mut p, CMD_RESET_BAT, rx).map(|()| p[0]))
    } else {
        None
    };
    let mut param = [ID_INVALID, ID_INVALID];
    let id = match dev.command(c, &mut param, CMD_GETID, rx) {
        Err(_) => {
            let mut p = [0u8; 1];
            dev.command(c, &mut p, CMD_SETLEDS, rx).map_err(KbdFail::Command)?;
            // atkbd.c:846-849 returns at once: no deactivate on this path.
            return Ok(Probe { id: ID_LEDS_ONLY, reset: reset_outcome, deactivate: Ok(()) });
        }
        Ok(()) => {
            if !is_keyboard_id(param[0]) {
                return Err(KbdFail::NotAKeyboard { id0: param[0] });
            }
            ((param[0] as u16) << 8) | param[1] as u16
        }
    };
    if id == ID_NCD && translated {
        return Err(KbdFail::NcdOnTranslatingController);
    }
    let deactivate = dev.command(c, &mut [], CMD_RESET_DIS, rx);
    Ok(Probe { id, reset: reset_outcome, deactivate })
}

/// `atkbd_select_set` (atkbd.c:881-940) for `target_set` 2, the default (`atkbd_set = 2`, atkbd.c:36), with
/// `atkbd_terminal` false. Returns the set the decoder must speak and whether extended keys are on.
pub fn select_set<P: Port>(
    dev: &mut Ps2Dev,
    c: &mut Controller<P>,
    id: u16,
    translated: bool,
    target_set: u8,
    allow_extra: bool,
    rx: &mut dyn FnMut(u8),
) -> Result<(u8, bool), KbdFail> {
    if translated {
        return Ok((2, false));
    }
    if id == ID_NCD {
        let mut p = [3u8];
        let _ = dev.command(c, &mut p, CMD_SSCANSET, rx);
        return Ok((3, false));
    }
    if allow_extra {
        let mut p = [EX_ENABLE_PARAM];
        if dev.command(c, &mut p, CMD_EX_ENABLE, rx).is_ok() {
            return Ok((2, true));
        }
    }
    if target_set != 3 {
        return Ok((2, false));
    }
    Err(KbdFail::Set3NotPorted)
}

/// The typematic byte for a delay and period, and the delay/period the keyboard will actually use
/// (`atkbd_set_repeat_rate`'s table walk, atkbd.c:595-603).
pub fn repeat_param(delay_ms: u16, period_ms: u16) -> (u8, u16, u16) {
    let mut i = 0usize;
    while i < REPEAT_PERIODS_MS.len() - 1 && REPEAT_PERIODS_MS[i] < period_ms {
        i += 1;
    }
    let mut j = 0usize;
    while j < REPEAT_DELAYS_MS.len() - 1 && REPEAT_DELAYS_MS[j] < delay_ms {
        j += 1;
    }
    ((i as u8) | ((j as u8) << 5), REPEAT_DELAYS_MS[j], REPEAT_PERIODS_MS[i])
}

/// `atkbd_set_repeat_rate` (atkbd.c:583-605). Returns the byte sent.
pub fn set_repeat_rate<P: Port>(
    dev: &mut Ps2Dev,
    c: &mut Controller<P>,
    delay_ms: u16,
    period_ms: u16,
    rx: &mut dyn FnMut(u8),
) -> Result<u8, KbdFail> {
    let (param, _, _) = repeat_param(delay_ms, period_ms);
    let mut p = [param];
    dev.command(c, &mut p, CMD_SETREP, rx).map_err(KbdFail::Command)?;
    Ok(param)
}

/// `atkbd_set_leds` (atkbd.c:607-631), the plain three (no `atkbd->extra` keyboard).
pub fn set_leds<P: Port>(dev: &mut Ps2Dev, c: &mut Controller<P>, leds: u8, rx: &mut dyn FnMut(u8)) -> Result<(), KbdFail> {
    let mut p = [leds & (LED_SCROLL | LED_NUM | LED_CAPS)];
    dev.command(c, &mut p, CMD_SETLEDS, rx).map_err(KbdFail::Command)
}

/// `atkbd_activate` (atkbd.c:733-749).
pub fn activate<P: Port>(dev: &mut Ps2Dev, c: &mut Controller<P>, rx: &mut dyn FnMut(u8)) -> Result<(), KbdFail> {
    dev.command(c, &mut [], CMD_ENABLE, rx).map_err(KbdFail::Command)
}
