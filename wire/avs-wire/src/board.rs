// SPDX-License-Identifier: MIT OR Apache-2.0
//! The BOARD RECORD: the facts about one board that avs.device must not hard-code (ZERO-HW-OUTSIDE-BSP) - the codec's and the speaker
//! amplifiers' I2C addresses, which SSP the headset is on and which the speakers are on, the headphone stream, the codec clock, the I2C
//! controller's timing, and the two GPIO pads (the codec's interrupt, the amplifiers' reset). The CoralOS loader writes it from the
//! board's launch row; the device asks for it on the firmware channel as the file [`BOARD_FILE`] and refuses to start without it.
//!
//! The Atlas values are in the launch row and in tests, not in a driver: codec 0x1A, amplifiers 0x31 / 0x32, headset on SSP1, speakers on
//! SSP0, MCLK 24.576 MHz, jack interrupt GPP_F10, amplifier reset GPP_F11 (ATLAS-PERIPHERALS-FROM-RECOVERY.md section 3.2).
use crate::codec::{Reader, Writer};
use crate::WireError;

pub const BOARD_FILE: &str = "board.bin";
pub const BOARD_MAGIC: u32 = u32::from_le_bytes(*b"AVSB");
pub const BOARD_VERSION: u16 = 1;
/// How many amplifier addresses / speaker SSPs a record can list: a limit of the record's layout.
pub const MAX_LISTED: usize = 4;

/// A pad: which community window of the pad controller, and the pad's index within that community.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pad {
    pub community: u8,
    pub pad: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Board {
    /// The PCI function the row binds (cAVS: 8086:9d71).
    pub pci_vendor: u16,
    pub pci_device: u16,
    pub codec_addr: u16,
    pub amps: [u16; MAX_LISTED],
    pub n_amps: u8,
    pub headset_ssp: u8,
    pub speaker_ssp: [u8; MAX_LISTED],
    pub n_speaker_ssp: u8,
    /// Which playback stream (0-based) carries the headphone path.
    pub playback_stream: u8,
    /// The codec's master clock, from the board.
    pub mclk_hz: u32,
    /// The I2C bus the codec is on: its speed and the controller's fast-mode counts (from the row / ACPI `FMCN`), 0 = leave as the firmware set.
    pub i2c_bus_hz: u32,
    pub i2c_fs_hcnt: u16,
    pub i2c_fs_lcnt: u16,
    pub i2c_sda_hold: u32,
    /// The codec's interrupt pad. The line is asserted LOW (level) on Atlas.
    pub jack_pad: Pad,
    /// The speaker amplifiers' reset pad (`SPKR_RST_L`, active low): driven low to HOLD them in reset.
    pub spkr_rst_pad: Pad,
    /// The offset in each pad window of the pad configuration registers is read from the window itself (`PADBAR`); these two are the
    /// communities' window lengths the loader granted, so a pad outside them is refused at load.
    pub pad_window_len: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardError {
    Wire(WireError),
    /// The codec's address is one of the speaker amplifiers'.
    CodecIsAnAmplifier { addr: u16 },
    /// The headset is on one of the speakers' SSPs.
    HeadsetOnSpeakerSsp { ssp: u8 },
    /// The amplifiers' list is empty or the codec's address is not a 7-bit address.
    BadAddress { addr: u16 },
    /// No speaker-reset pad is named for a board that lists amplifiers (the safe state has nothing to hold).
    NoSpeakerResetPad,
    /// The same pad is both the interrupt and the reset.
    PadsCollide,
}

impl BoardError {
    pub const fn name(&self) -> &'static str {
        match self {
            BoardError::Wire(_) => "board-wire",
            BoardError::CodecIsAnAmplifier { .. } => "codec-is-an-amplifier",
            BoardError::HeadsetOnSpeakerSsp { .. } => "headset-on-speaker-ssp",
            BoardError::BadAddress { .. } => "bad-address",
            BoardError::NoSpeakerResetPad => "no-speaker-reset-pad",
            BoardError::PadsCollide => "pads-collide",
        }
    }
}

impl From<WireError> for BoardError {
    fn from(e: WireError) -> Self {
        BoardError::Wire(e)
    }
}

impl Board {
    pub fn amps(&self) -> &[u16] {
        &self.amps[..self.n_amps as usize]
    }
    pub fn speaker_ssps(&self) -> &[u8] {
        &self.speaker_ssp[..self.n_speaker_ssp as usize]
    }

    /// The consistency every consumer relies on, checked once at load.
    pub fn check(&self) -> Result<(), BoardError> {
        if self.codec_addr == 0 || self.codec_addr > 0x77 {
            return Err(BoardError::BadAddress { addr: self.codec_addr });
        }
        for a in self.amps() {
            if *a == 0 || *a > 0x77 {
                return Err(BoardError::BadAddress { addr: *a });
            }
        }
        if self.amps().contains(&self.codec_addr) {
            return Err(BoardError::CodecIsAnAmplifier { addr: self.codec_addr });
        }
        if self.speaker_ssps().contains(&self.headset_ssp) {
            return Err(BoardError::HeadsetOnSpeakerSsp { ssp: self.headset_ssp });
        }
        if !self.amps().is_empty() && self.spkr_rst_pad.pad == u16::MAX {
            return Err(BoardError::NoSpeakerResetPad);
        }
        if self.jack_pad == self.spkr_rst_pad {
            return Err(BoardError::PadsCollide);
        }
        Ok(())
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u32("board.magic", BOARD_MAGIC)?;
        w.u16("board.version", BOARD_VERSION)?;
        w.u16("board.pci_vendor", self.pci_vendor)?;
        w.u16("board.pci_device", self.pci_device)?;
        w.u16("board.codec_addr", self.codec_addr)?;
        w.u8("board.n_amps", self.n_amps)?;
        for a in &self.amps {
            w.u16("board.amp", *a)?;
        }
        w.u8("board.headset_ssp", self.headset_ssp)?;
        w.u8("board.n_speaker_ssp", self.n_speaker_ssp)?;
        w.u8("board.playback_stream", self.playback_stream)?;
        for s in &self.speaker_ssp {
            w.u8("board.speaker_ssp", *s)?;
        }
        w.u8("board.reserved", 0)?;
        w.u8("board.reserved", 0)?;
        w.u32("board.mclk_hz", self.mclk_hz)?;
        w.u32("board.i2c_bus_hz", self.i2c_bus_hz)?;
        w.u16("board.i2c_fs_hcnt", self.i2c_fs_hcnt)?;
        w.u16("board.i2c_fs_lcnt", self.i2c_fs_lcnt)?;
        w.u32("board.i2c_sda_hold", self.i2c_sda_hold)?;
        for p in [self.jack_pad, self.spkr_rst_pad] {
            w.u8("board.pad_community", p.community)?;
            w.u16("board.pad_index", p.pad)?;
        }
        w.u16("board.pad_window_len_lo", self.pad_window_len as u16)?;
        w.u16("board.pad_window_len_hi", (self.pad_window_len >> 16) as u16)?;
        Ok(w.pos())
    }

    pub fn decode(b: &[u8]) -> Result<Self, BoardError> {
        let mut r = Reader::new(b);
        let magic = r.u32("board.magic")?;
        if magic != BOARD_MAGIC {
            return Err(WireError::BadMagic { got: magic }.into());
        }
        let ver = r.u16("board.version")?;
        if ver != BOARD_VERSION {
            return Err(WireError::BadMajor { got: ver }.into());
        }
        let pci_vendor = r.u16("board.pci_vendor")?;
        let pci_device = r.u16("board.pci_device")?;
        let codec_addr = r.u16("board.codec_addr")?;
        let n_amps = r.u8("board.n_amps")?;
        let mut amps = [0u16; MAX_LISTED];
        for a in amps.iter_mut() {
            *a = r.u16("board.amp")?;
        }
        let headset_ssp = r.u8("board.headset_ssp")?;
        let n_speaker_ssp = r.u8("board.n_speaker_ssp")?;
        let playback_stream = r.u8("board.playback_stream")?;
        let mut speaker_ssp = [0u8; MAX_LISTED];
        for s in speaker_ssp.iter_mut() {
            *s = r.u8("board.speaker_ssp")?;
        }
        let (r1, r2) = (r.u8("board.reserved")?, r.u8("board.reserved")?);
        if r1 != 0 || r2 != 0 {
            return Err(WireError::ReservedNonZero { what: "board.reserved", got: (r1 as u64) << 8 | r2 as u64 }.into());
        }
        if n_amps as usize > MAX_LISTED || n_speaker_ssp as usize > MAX_LISTED {
            return Err(WireError::LimitExceeded { what: "board.count", got: n_amps.max(n_speaker_ssp) as u64, max: MAX_LISTED as u64 }.into());
        }
        let mclk_hz = r.u32("board.mclk_hz")?;
        let i2c_bus_hz = r.u32("board.i2c_bus_hz")?;
        let i2c_fs_hcnt = r.u16("board.i2c_fs_hcnt")?;
        let i2c_fs_lcnt = r.u16("board.i2c_fs_lcnt")?;
        let i2c_sda_hold = r.u32("board.i2c_sda_hold")?;
        let jack_pad = Pad { community: r.u8("board.pad_community")?, pad: r.u16("board.pad_index")? };
        let spkr_rst_pad = Pad { community: r.u8("board.pad_community")?, pad: r.u16("board.pad_index")? };
        let lo = r.u16("board.pad_window_len_lo")? as u32;
        let hi = r.u16("board.pad_window_len_hi")? as u32;
        r.finish()?;
        let s = Self {
            pci_vendor,
            pci_device,
            codec_addr,
            amps,
            n_amps,
            headset_ssp,
            speaker_ssp,
            n_speaker_ssp,
            playback_stream,
            mclk_hz,
            i2c_bus_hz,
            i2c_fs_hcnt,
            i2c_fs_lcnt,
            i2c_sda_hold,
            jack_pad,
            spkr_rst_pad,
            pad_window_len: lo | hi << 16,
        };
        s.check()?;
        Ok(s)
    }
}
