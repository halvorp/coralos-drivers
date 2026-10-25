// SPDX-License-Identifier: GPL-2.0-only
//! THE SPEAKER PATH'S HARD GAIN CEILING (DECISIONS D17). No client request, however large, produces a DSP gain above [`CEILING_LINEAR`]; the
//! device applies the number this module returns and nothing else, and the boot orchestrator refuses any peakvol request above the same
//! number a second time (`avs-boot-core` `PeakvolHandle`).
//!
//! WHERE THE NUMBER COMES FROM. The Pixelbook Go ships with the Maxim DSM speaker-protection library: it watches the amplifier's voltage and
//! current feedback and keeps the cones inside their excursion and thermal limits. This path has NO DSM (the upstream topology carries none;
//! DSM is the named later upgrade, D17), so the ceiling is how far below ChromeOS's own maximum we stay while nothing watches the speaker.
//!
//! 1. ChromeOS's maximum. `etc/cras/avs_max98373` of the recovery image is an `explicit` volume curve for the built-in speaker; step 100
//!    is `db_at_100 = 0`: CRAS puts full scale (0 dBFS) into the DSP, whose `DSP Volume` ChromeOS leaves alone (the UCM sets none), and the
//!    amplifier is at its UCM output voltage (9.65 V, `Output Voltage 5`). That 0 dB is reached ONLY because DSM is there to catch it.
//! 2. What the amplifier's own protections do. DHT (dynamic headroom tracking) and BDE (brownout) protect the part and the battery. Neither
//!    knows the speaker's excursion. They are on (the UCM set), and they do not replace the ceiling.
//! 3. The ceiling. Full scale into 9.65 V peaks at 9.65 V; a sine at peak amplitude `V` dissipates `V^2 / (2R)`. At 0 dB that is 5.8 W into
//!    8 ohm and 11.6 W into 4 ohm - a laptop speaker is rated roughly 1 to 2 W. The CRAS step at which the curve first reaches -12.00 dB is
//!    step 69 (`db_at_69 = -1200`; `db_at_70` and `db_at_71` are -1150, `db_at_68` is -1250 so 69 is the highest step at or below -12.00 dB).
//!    At -12.00 dB the peak is 2.43 V: 0.37 W into 8 ohm, 0.74 W into 4 ohm, under any plausible rating with a factor of two to spare, and
//!    28 dB above the -40 dBFS test tone the owner called clearly audible (440 Hz, -20 dBFS source, DSP volume 10%, 2026-10-16).
//!    [`CEILING_STEP`] is that step, [`CEILING_CDB`] its curve value, [`CEILING_LINEAR`] its Q1.31 gain (`S32_MAX` is unity, the unit of
//!    the peakvol module). It is also below the topology's own `DSP Volume` control maximum, `0x3FFFFFFF` (-6.02 dB).
//!
//! THE SPEAKER RATING IS NOT IN THE EXTRACT. The 8 ohm / 4 ohm figures are the part's two supported loads, not a measurement; the owner's
//! first listening tests at a low level, and a measurement of the real cone, replace this paragraph. delete-when: DSM runs (`dsm_core` and
//! `DSM data`), then the ceiling is the curve's own maximum again.
//!
//! THE MAPPING. Aura's level 0..=[`LEVEL_MAX`] (the wire's `VOLUME_MAX`) maps to [`STEPS`] steps of 0.75 dB below the ceiling (the headphone
//! path's own spacing, `codec::DAC_RANGE_CODES`): level `LEVEL_MAX` is the ceiling, level 1 is 47.25 dB below it, level 0 or mute is
//! silence (gain 0). [`GAIN_Q31`] is the table; the test recomputes every entry from `10^(dB/20)`.

/// The wire's top level (`avs_wire::VOLUME_MAX`); restated here so this crate has no dependency. A test pins the two together on the
/// device's side.
pub const LEVEL_MAX: u16 = 1000;
/// Steps of 0.75 dB between level 1 and the ceiling, plus the ceiling itself.
pub const STEPS: usize = 64;
/// The spacing of the steps, in hundredths of a dB.
pub const STEP_CDB: i32 = 75;

/// ChromeOS's CRAS explicit volume curve for the built-in speaker, hundredths of a dB, index = volume step 0..=100
/// (`etc/cras/avs_max98373` `db_at_<step>`).
pub const CRAS_CURVE_CDB: [i16; 101] = [
    -4600, -4500, -4400, -4350, -4250, -4150, -4050, -4000, -3900, -3800,
    -3700, -3650, -3550, -3450, -3400, -3300, -3250, -3200, -3150, -3100,
    -3050, -3000, -2950, -2900, -2850, -2800, -2750, -2700, -2650, -2600,
    -2550, -2550, -2500, -2450, -2400, -2400, -2350, -2300, -2250, -2250,
    -2200, -2150, -2100, -2100, -2050, -2000, -1950, -1950, -1900, -1850,
    -1800, -1800, -1750, -1700, -1700, -1650, -1600, -1600, -1550, -1500,
    -1500, -1450, -1400, -1400, -1350, -1350, -1300, -1250, -1250, -1200,
    -1150, -1150, -1100, -1050, -1050, -1000, -1000, -950, -900, -900,
    -850, -800, -800, -750, -750, -700, -650, -650, -600, -550,
    -500, -500, -450, -400, -350, -300, -250, -200, -100, -50,
    0,
];

/// The CRAS step the ceiling is: the highest step whose curve value is at or below -12.00 dB (see the module comment).
pub const CEILING_STEP: usize = 69;
pub const CEILING_CDB: i16 = CRAS_CURVE_CDB[CEILING_STEP];
/// The ceiling as the DSP's linear Q1.31 gain: `floor(S32_MAX * 10^(CEILING_CDB / 2000))`. Nothing ever asks the peakvol module for more.
pub const CEILING_LINEAR: u32 = 539423503; // 0x2026f30f

/// Q1.31 gain at `i` steps of 0.75 dB below the ceiling, `i` in `0..STEPS`: `floor(S32_MAX * 10^((CEILING_CDB/100 - 0.75 i) / 20))`.
pub const GAIN_Q31: [u32; STEPS] = [
    539423503, 494800198, 453868315, 416322483,
    381882595, 350291714, 321314160, 294733746,
    270352173, 247987542, 227473005, 208655513,
    191394681, 175561735, 161038555, 147716791,
    135497057, 124288190, 114006566, 104575479,
    95924570, 87989300, 80710467, 74033769,
    67909395, 62291654, 57138635, 52411895,
    48076170, 44099114, 40451056, 37104780,
    34035322, 31219781, 28637154, 26268172,
    24095162, 22101913, 20273552, 18596442,
    17058068, 15646956, 14352576, 13165272,
    12076187, 11077196, 10160845, 9320299,
    8549286, 7842054, 7193328, 6598266,
    6052431, 5551749, 5092486, 4671215,
    4284793, 3930337, 3605204, 3306967,
    3033401, 2782465, 2552289, 2341153,
];

/// How many 0.75 dB steps below the ceiling a level is (`0..STEPS`); level 0 counts as the quietest step. Levels above [`LEVEL_MAX`] are
/// [`LEVEL_MAX`].
pub const fn steps_below(level: u16) -> usize {
    let level = if level > LEVEL_MAX { LEVEL_MAX } else { level } as u32;
    let below = ((LEVEL_MAX as u32 - level) * STEPS as u32 / LEVEL_MAX as u32) as usize;
    if below >= STEPS {
        STEPS - 1
    } else {
        below
    }
}

/// The DSP gain for a client's level. Level 0 or `mute` is silence. Any level above [`LEVEL_MAX`] is treated as [`LEVEL_MAX`]. The result is
/// never above [`CEILING_LINEAR`], for any input: this is the only function the device derives a speaker gain from.
pub const fn linear_for(level: u16, mute: bool) -> u32 {
    if mute || level == 0 {
        return 0;
    }
    clamp(GAIN_Q31[steps_below(level)])
}

/// A gain no larger than the ceiling. The second line of defence: every gain that leaves this crate passes it.
pub const fn clamp(linear: u32) -> u32 {
    if linear > CEILING_LINEAR {
        CEILING_LINEAR
    } else {
        linear
    }
}
