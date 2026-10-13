// SPDX-License-Identifier: GPL-2.0-only
//! DAI format, sample rate and word/channel configuration.
mod common;
use common::*;
use da7219_core::dai::*;

const FORMATS: [Format; 4] = [Format::I2s, Format::LeftJ, Format::RightJ, Format::DspB];
const INVS: [Inversion; 4] = [Inversion::NbNf, Inversion::NbIf, Inversion::IbNf, Inversion::IbIf];

#[test]
fn every_format_and_inversion_gives_the_linux_table() {
    const WCLK: u8 = 0x08; // DAI_WCLK_POL_INV, da7219.h:248
    const BCLK: u8 = 0x04; // DAI_CLK_POL_INV, da7219.h:245
    // da7219.c:1326-1365, rows in INVS order: NB_NF, NB_IF, IB_NF, IB_IF
    let std_rows = [0, WCLK, BCLK, WCLK | BCLK];
    let dsp_rows = [BCLK, WCLK | BCLK, 0, WCLK];
    let fmt_bits = [0x00u8, 0x01, 0x02, 0x03]; // I2S, LEFT_J, RIGHT_J, DSP (da7219.h:257-260)
    for (fi, f) in FORMATS.iter().enumerate() {
        for (ii, inv) in INVS.iter().enumerate() {
            let want_clk = if *f == Format::DspB { dsp_rows[ii] } else { std_rows[ii] };
            let got = fmt_bits_of(*f, *inv);
            assert_eq!(got, (want_clk, fmt_bits[fi]), "{f:?} {inv:?}");
        }
    }
}

fn fmt_bits_of(format: Format, inversion: Inversion) -> (u8, u8) {
    da7219_core::dai::fmt_bits(DaiFmt { format, inversion, role: Role::Consumer })
}

#[test]
fn atlas_is_i2s_normal_clocks_with_the_codec_as_consumer() {
    assert_eq!(ATLAS_DAI_FMT, DaiFmt { format: Format::I2s, inversion: Inversion::NbNf, role: Role::Consumer });
    let mut f = Fake::reset();
    f.regs[0x2b] = 0x0c; // pretend both polarities were inverted
    f.regs[0x2c] = 0x03;
    f.run(fmt_plan(ATLAS_DAI_FMT).unwrap()).unwrap();
    assert_eq!(f.reg(0x2b) & 0x0c, 0);
    assert_eq!(f.reg(0x2c) & 0x03, 0);
    assert_only_codec(&f);
}

#[test]
fn provider_mode_is_refused_by_name() {
    assert_eq!(fmt_plan(DaiFmt { format: Format::I2s, inversion: Inversion::NbNf, role: Role::Provider }), Err(DaiError::ProviderNotSupported));
}

#[test]
fn every_sample_rate_has_the_linux_code_and_others_are_refused() {
    // da7219.h:177-187
    let table = [(8000, 0x01u8), (11025, 0x02), (12000, 0x03), (16000, 0x05), (22050, 0x06), (24000, 0x07), (32000, 0x09), (44100, 0x0a), (48000, 0x0b), (88200, 0x0e), (96000, 0x0f)];
    for (hz, code) in table {
        assert_eq!(sr_code(hz), Ok(code), "{hz}");
    }
    for hz in [0, 7999, 47999, 176400, 192000] {
        assert_eq!(sr_code(hz), Err(DaiError::UnsupportedRate { hz }));
    }
}

#[test]
fn word_length_and_channel_bits_are_the_linux_ones() {
    // DAI_CTRL: word length bits 2-3 (da7219.h:263-266), channel count shifted 4 (da7219.h:267)
    assert_eq!(word_and_channels(16, 2), Ok(0x00 | 0x20));
    assert_eq!(word_and_channels(20, 2), Ok(0x04 | 0x20));
    assert_eq!(word_and_channels(24, 2), Ok(0x08 | 0x20));
    assert_eq!(word_and_channels(32, 2), Ok(0x0c | 0x20));
    assert_eq!(word_and_channels(32, 1), Ok(0x0c | 0x10));
    assert_eq!(word_and_channels(8, 2), Err(DaiError::UnsupportedWidth { bits: 8 }));
    assert_eq!(word_and_channels(32, 0), Err(DaiError::UnsupportedChannels { channels: 0 }));
    assert_eq!(word_and_channels(32, 3), Err(DaiError::UnsupportedChannels { channels: 3 }));
}

#[test]
fn hw_params_for_48k_stereo_32_bit_writes_the_rate_then_the_word_bits_and_leaves_format_and_enable_alone() {
    let mut f = Fake::reset();
    f.regs[0x2c] = 0x80 | 0x01; // DAI_EN and a non-I2S format must survive
    f.run(hw_params_plan(48000, 32, 2).unwrap()).unwrap();
    assert_eq!(f.reg(0x17), 0x0b);
    assert_eq!(f.reg(0x2c), 0x80 | 0x01 | 0x0c | 0x20);
    assert_eq!(f.writes.iter().map(|w| w.1).collect::<Vec<_>>(), vec![0x17, 0x2c]);
    assert_eq!(hw_params_plan(47000, 32, 2), Err(DaiError::UnsupportedRate { hz: 47000 }));
}
