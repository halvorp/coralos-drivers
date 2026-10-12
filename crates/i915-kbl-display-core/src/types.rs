// SPDX-License-Identifier: GPL-2.0-only
//! The small enums every other module speaks: pipes, transcoders, DDI ports.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_display_limits.h (enum pipe, enum transcoder, enum port).
//! Copyright (c) Intel Corporation.

/// `enum pipe`: PIPE_A = 0 ... (intel_display_limits.h:14-24). Kaby Lake has three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Pipe {
    A = 0, // intel_display_limits.h:17
    B = 1, // intel_display_limits.h:18
    C = 2, // intel_display_limits.h:19
}

impl Pipe {
    pub const ALL: [Pipe; 3] = [Pipe::A, Pipe::B, Pipe::C];
    pub const fn index(self) -> usize {
        self as usize
    }
    pub const fn letter(self) -> char {
        match self {
            Pipe::A => 'A',
            Pipe::B => 'B',
            Pipe::C => 'C',
        }
    }
}

/// `enum transcoder`: TRANSCODER_A/B/C equal their pipes; TRANSCODER_EDP comes after TRANSCODER_D
/// (intel_display_limits.h:34-43), so its enum value is 4 and `1 << 4` is its bit in the enabled-transcoder mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Transcoder {
    A = 0,   // intel_display_limits.h:34
    B = 1,   // intel_display_limits.h:35
    C = 2,   // intel_display_limits.h:36
    Edp = 4, // intel_display_limits.h:43 (TRANSCODER_D = 3 sits before it)
}

impl Transcoder {
    pub const fn name(self) -> &'static str {
        match self {
            Transcoder::A => "A",
            Transcoder::B => "B",
            Transcoder::C => "C",
            Transcoder::Edp => "EDP",
        }
    }
    /// The bit this transcoder holds in Linux's `enabled_transcoders` mask (`BIT(cpu_transcoder)`).
    pub const fn mask_bit(self) -> u8 {
        1 << (self as u8)
    }
    /// The transcoder that shares its enum value with `pipe` (TRANSCODER_A = PIPE_A ...).
    pub const fn of_pipe(pipe: Pipe) -> Transcoder {
        match pipe {
            Pipe::A => Transcoder::A,
            Pipe::B => Transcoder::B,
            Pipe::C => Transcoder::C,
        }
    }
}

/// `enum port` for the DDI ports of display ver 9: PORT_A = 0 ... PORT_E (intel_display_limits.h:95-99).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Ddi {
    A = 0, // intel_display_limits.h:95
    B = 1, // intel_display_limits.h:96
    C = 2, // intel_display_limits.h:97
    D = 3, // intel_display_limits.h:98
    E = 4, // intel_display_limits.h:99
}

impl Ddi {
    pub const ALL: [Ddi; 5] = [Ddi::A, Ddi::B, Ddi::C, Ddi::D, Ddi::E];
    pub const fn index(self) -> usize {
        self as usize
    }
    pub const fn letter(self) -> char {
        match self {
            Ddi::A => 'A',
            Ddi::B => 'B',
            Ddi::C => 'C',
            Ddi::D => 'D',
            Ddi::E => 'E',
        }
    }
    pub const fn from_index(i: u32) -> Option<Ddi> {
        match i {
            0 => Some(Ddi::A),
            1 => Some(Ddi::B),
            2 => Some(Ddi::C),
            3 => Some(Ddi::D),
            4 => Some(Ddi::E),
            _ => None,
        }
    }
}
