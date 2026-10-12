// SPDX-License-Identifier: GPL-2.0-only
//! A host stream of the controller (a stream descriptor in BAR0): its identity, the stream format word, and the plans that prepare, start,
//! stop and release it in the order the AVS PCM front end runs them (`avs_dai_fe_prepare` pcm.c:744-785, `avs_dai_fe_trigger`
//! pcm.c:861-925, `avs_dai_fe_hw_free`).
//!
//! Stream indexes and tags follow `avs_hdac_bus_init_streams` (avs/core.c:90-103) and `snd_hdac_ext_stream_init_all`
//! (ext/stream.c:90-121): the controller has `cp` input (capture) streams then `pb` output (playback) streams, indexes 0.., and tags count
//! from 1 within EACH direction. The DSP's DMA id for a host gateway is `stream_tag - 1` (pcm.c:`hdac_stream(host_stream)->stream_tag - 1`).
use crate::bdl::Bdl;
use crate::consts::*;
use crate::controller::hda;
use crate::error::HdaError;
use crate::regs::*;
use alloc::vec::Vec;
use avs_fw_core::effect::{Op, OnFail, PollSpec, Reg, Space, Step, Width};
use avs_fw_core::runner::Program;

/// Stream counts from `GCAP` (avs/core.c:94-98): input streams in bits 11:8, output streams in bits 15:12.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Streams {
    pub capture: u32,
    pub playback: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Capture,
    Playback,
}

/// One stream descriptor: where it is and the tag the DMA and the DSP know it by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostStream {
    pub index: u32,
    pub tag: u32,
    pub direction: Direction,
}

impl HostStream {
    /// The DSP's DMA id of this stream's host gateway (pcm.c: `stream_tag - 1`).
    pub const fn dma_id(&self) -> u32 {
        self.tag - 1
    }
}

impl Streams {
    pub const fn from_gcap(gcap: u16) -> Self {
        Self { capture: ((gcap >> 8) & 0x0f) as u32, playback: ((gcap >> 12) & 0x0f) as u32 } // avs/core.c:96-97
    }
    pub const fn total(&self) -> u32 {
        self.capture + self.playback
    }
    /// The `n`-th playback stream (0-based): index `capture + n`, tag `n + 1` (avs/core.c:101, ext/stream.c:95-107).
    pub fn playback_stream(&self, n: u32) -> Result<HostStream, HdaError> {
        if n >= self.playback {
            return Err(HdaError::StreamIndexOutOfRange { index: self.capture + n, streams: self.total() });
        }
        Ok(HostStream { index: self.capture + n, tag: n + 1, direction: Direction::Playback })
    }
    /// The `n`-th capture stream: index `n`, tag `n + 1` (avs/core.c:100).
    pub fn capture_stream(&self, n: u32) -> Result<HostStream, HdaError> {
        if n >= self.capture {
            return Err(HdaError::StreamIndexOutOfRange { index: n, streams: self.total() });
        }
        Ok(HostStream { index: n, tag: n + 1, direction: Direction::Capture })
    }
}

/// A register of stream descriptor `index` (stream.c:98: `0x20 * idx + 0x80`).
pub fn sd_reg(index: u32, reg: u32, width: Width) -> Reg {
    Reg { space: Space::Hda, off: SD_BASE + SD_INTERVAL * index + reg, width }
}

/// `AZX_PPCTL_PROCEN(x)` (hda_register.h:228).
pub const fn ppctl_procen(index: u32) -> u32 {
    1 << index // hda_register.h:228
}

/// `hext_stream->pphc_addr` (ext/stream.c:70-71): the processing-pipe host-channel block of stream `index`.
pub const fn pphc_base(pp: u32, index: u32) -> u32 {
    pp + AZX_PPHC_BASE + AZX_PPHC_INTERVAL * index
}

/// `hext_stream->pplc_addr` (ext/stream.c:73-75): the processing-pipe link-channel block of stream `index`.
pub const fn pplc_base(pp: u32, num_streams: u32, index: u32) -> u32 {
    pp + AZX_PPLC_BASE + AZX_PPLC_MULTI * num_streams + AZX_PPLC_INTERVAL * index
}

/// `snd_hdac_stream_format(channels, bits, rate)` (device.c:785-825): the `SDnFMT` word, or the named refusal where Linux returns 0.
pub fn stream_format(channels: u32, bits: u32, rate: u32) -> Result<u32, HdaError> {
    // (rate, HDA_RATE(base, mult, div)) - device.c:706-716, :698-700; 9600 Hz (device.c:721) is "not autodetected" and is omitted
    const RATES: [(u32, u32); 11] = [
        (8000, rate_word(AC_FMT_BASE_48K, 1, 6)),
        (11025, rate_word(AC_FMT_BASE_44K, 1, 4)),
        (16000, rate_word(AC_FMT_BASE_48K, 1, 3)),
        (22050, rate_word(AC_FMT_BASE_44K, 1, 2)),
        (32000, rate_word(AC_FMT_BASE_48K, 2, 3)),
        (44100, rate_word(AC_FMT_BASE_44K, 1, 1)),
        (48000, rate_word(AC_FMT_BASE_48K, 1, 1)),
        (88200, rate_word(AC_FMT_BASE_44K, 2, 1)),
        (96000, rate_word(AC_FMT_BASE_48K, 2, 1)),
        (176400, rate_word(AC_FMT_BASE_44K, 4, 1)),
        (192000, rate_word(AC_FMT_BASE_48K, 4, 1)),
    ];
    let bad = HdaError::InvalidStreamFormat { rate, channels, bits };
    let mut val = RATES.iter().find(|r| r.0 == rate).map(|r| r.1).ok_or(bad)?;
    if channels == 0 || channels > 16 {
        return Err(bad); // device.c:800-801
    }
    val |= channels - 1; // device.c:802
    val |= match bits {
        8 => AC_FMT_BITS_8,
        16 => AC_FMT_BITS_16,
        20 => AC_FMT_BITS_20,
        24 => AC_FMT_BITS_24,
        32 => AC_FMT_BITS_32,
        _ => return Err(bad), // device.c:804-823
    };
    Ok(val)
}

/// `HDA_RATE(base, mult, div)` (device.c:698-700).
const fn rate_word(base: u32, mult: u32, div: u32) -> u32 {
    base | ((mult - 1) << AC_FMT_MULT_SHIFT) | ((div - 1) << AC_FMT_DIV_SHIFT)
}

fn check(streams: &Streams, index: u32) -> Result<(), HdaError> {
    if index >= streams.total() {
        return Err(HdaError::StreamIndexOutOfRange { index, streams: streams.total() });
    }
    Ok(())
}

fn a(label: &'static str, op: Op) -> Step {
    Step::new(label, op, OnFail::Abort)
}

/// `snd_hdac_stream_clear` (stream.c:167-176): stop the DMA and its interrupt enables, clear the status.
fn clear_steps(p: &mut Program, index: u32) {
    p.push(a("sd-dma-stop", Op::Update { reg: sd_reg(index, AZX_REG_SD_CTL, Width::B8), mask: SD_CTL_DMA_START | SD_INT_MASK, value: 0 }));
    p.push(a("sd-sts-clear", Op::Write { reg: sd_reg(index, AZX_REG_SD_STS, Width::B8), value: SD_INT_MASK }));
}

/// `snd_hdac_ext_stream_decouple` (ext/stream.c:137-165): the processing-pipe enable bit of the stream, set while it is prepared.
pub fn decouple(streams: &Streams, pp: u32, index: u32, decouple: bool) -> Result<Program, HdaError> {
    check(streams, index)?;
    let mask = ppctl_procen(index);
    let mut p = Program::new();
    p.push(a(if decouple { "ppctl-decouple" } else { "ppctl-couple" }, Op::Update { reg: hda(pp + AZX_REG_PP_PPCTL, Width::D32), mask, value: if decouple { mask } else { 0 } }));
    Ok(p)
}

/// `snd_hdac_stream_reset` (stream.c:224-255): stop, set the stream reset bit, wait for it to read back, clear it, wait for it to clear.
/// Linux ignores both poll results; here each is a named failure (`sd-reset-entered`, `sd-reset-exited`). `dma_stop_delay` is 0 on the
/// AVS bus (never set), so the extra delay (stream.c:238-240) is not planned.
pub fn reset(streams: &Streams, index: u32) -> Result<Program, HdaError> {
    check(streams, index)?;
    let ctl = sd_reg(index, AZX_REG_SD_CTL, Width::B8);
    let mut p = Program::new();
    clear_steps(&mut p, index);
    p.push(a("sd-reset-enter", Op::Update { reg: ctl, mask: SD_CTL_STREAM_RESET, value: SD_CTL_STREAM_RESET }));
    p.push(a("sd-reset-entered", Op::Poll(PollSpec::new(ctl, SD_CTL_STREAM_RESET, SD_CTL_STREAM_RESET, STREAM_RESET_POLL_INTERVAL_US, STREAM_RESET_TIMEOUT_US))));
    p.push(a("sd-reset-exit", Op::Update { reg: ctl, mask: SD_CTL_STREAM_RESET, value: 0 }));
    p.push(a("sd-reset-exited", Op::Poll(PollSpec::new(ctl, SD_CTL_STREAM_RESET, 0, STREAM_RESET_POLL_INTERVAL_US, STREAM_RESET_TIMEOUT_US))));
    Ok(p)
}

/// What `snd_hdac_stream_setup` programs (stream.c:257-337).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetupParams {
    /// `SDnFMT` ([`stream_format`]).
    pub format: u32,
    /// `SD_CBL`: the cyclic buffer length in bytes.
    pub buffer_size: u32,
    /// `SD_LVI`: the last valid BDL index.
    pub lvi: u32,
    /// Bus address of the BDL page, 128-byte aligned.
    pub bdl_addr: u64,
    /// `bus->use_posbuf && bus->posbuf.addr` (AVS sets `use_posbuf`, avs/core.c:390).
    pub posbuf: bool,
}

impl SetupParams {
    pub fn new(format: u32, bdl: &Bdl, bdl_addr: u64, posbuf: bool) -> Self {
        Self { format, buffer_size: bdl.buffer_size, lvi: bdl.lvi(), bdl_addr, posbuf }
    }
}

/// `snd_hdac_stream_setup(code_loading = false)` (stream.c:257-337): the run bit off, the stream tag (and traffic priority - `bus->snoop`
/// is never set on the AVS bus, so `SD_CTL_TRAFFIC_PRIO` is set, stream.c:275-276), the buffer length, format, last valid index and BDL
/// address, the position buffer enable, the descriptor interrupt enables, and one read of the FIFO size.
///
/// DEVIATION, named: Linux polls `SD_FIFOSIZE` until non-zero for 300 us and merely logs a time-out (stream.c:306-314). A "non-zero"
/// wait is not expressible as an equality poll, and the FIFO size is informational (position delay), so the plan READS it once and
/// tolerates the result; the `.device` records the value.
pub fn setup(streams: &Streams, s: &HostStream, p: &SetupParams) -> Result<Program, HdaError> {
    check(streams, s.index)?;
    if s.tag == 0 || s.tag > STREAM_TAG_MAX {
        return Err(HdaError::StreamTagOutOfRange { tag: s.tag });
    }
    if p.bdl_addr & 0x7f != 0 {
        return Err(HdaError::UnalignedDmaAddress { addr: p.bdl_addr, align: 128 });
    }
    let i = s.index;
    let mut g = Program::new();
    clear_steps(&mut g, i);
    g.push(a("sd-stream-tag", Op::Update { reg: sd_reg(i, AZX_REG_SD_CTL, Width::D32), mask: SD_CTL_STREAM_TAG_MASK | SD_CTL_TRAFFIC_PRIO, value: (s.tag << SD_CTL_STREAM_TAG_SHIFT) | SD_CTL_TRAFFIC_PRIO }));
    g.push(a("sd-cbl", Op::Write { reg: sd_reg(i, AZX_REG_SD_CBL, Width::D32), value: p.buffer_size }));
    g.push(a("sd-format", Op::Write { reg: sd_reg(i, AZX_REG_SD_FORMAT, Width::W16), value: p.format }));
    g.push(a("sd-lvi", Op::Write { reg: sd_reg(i, AZX_REG_SD_LVI, Width::W16), value: p.lvi }));
    g.push(a("sd-bdlpl", Op::Write { reg: sd_reg(i, AZX_REG_SD_BDLPL, Width::D32), value: p.bdl_addr as u32 }));
    g.push(a("sd-bdlpu", Op::Write { reg: sd_reg(i, AZX_REG_SD_BDLPU, Width::D32), value: (p.bdl_addr >> 32) as u32 }));
    if p.posbuf {
        // Linux writes `addr | ENABLE` when ENABLE is clear; the address was programmed by init_chip, so setting the bit is the same write
        g.push(a("dplbase-enable", Op::Update { reg: hda(AZX_REG_DPLBASE, Width::D32), mask: AZX_DPLBASE_ENABLE, value: AZX_DPLBASE_ENABLE }));
    }
    g.push(a("sd-int-enable", Op::Update { reg: sd_reg(i, AZX_REG_SD_CTL, Width::D32), mask: SD_INT_MASK, value: SD_INT_MASK }));
    g.push(Step::new("sd-fifosize", Op::Read { reg: sd_reg(i, AZX_REG_SD_FIFOSIZE, Width::W16) }, OnFail::Tolerate));
    Ok(g)
}

/// `snd_hdac_stream_spbcap_enable` + `snd_hdac_stream_set_spib` (stream.c:787-830): enable the stream's SPIB register in the SPB
/// capability and write the software position in buffer. `spb` is [`crate::caps::Caps::spb`]. `spib` may not exceed the buffer (the
/// check Linux lacks; fw-core refuses the same for the code loader).
pub fn spib(streams: &Streams, spb: u32, index: u32, spib: u32, buffer_size: u32) -> Result<Program, HdaError> {
    check(streams, index)?;
    if spib > buffer_size {
        return Err(HdaError::SpibBeyondBuffer { spib, buffer: buffer_size });
    }
    let bit = 1u32 << index; // stream.c:798
    let mut p = Program::new();
    p.push(a("spbfcctl-enable", Op::Update { reg: hda(spb + AZX_REG_SPB_SPBFCCTL, Width::D32), mask: bit, value: bit }));
    p.push(a("spib-write", Op::Write { reg: hda(spb + AZX_SPB_BASE + AZX_SPB_INTERVAL * index + AZX_SPB_SPIB, Width::D32), value: spib }));
    Ok(p)
}

/// `snd_hdac_stream_start` (stream.c:130-165): the stream's interrupt enable in `INTCTL`, then the DMA run bit and the descriptor
/// interrupt enables (byte accesses, `access_sdnctl_in_dword` is off).
pub fn start(streams: &Streams, index: u32) -> Result<Program, HdaError> {
    check(streams, index)?;
    let mut p = Program::new();
    p.push(a("intctl-sie-on", Op::Update { reg: hda(AZX_REG_INTCTL, Width::D32), mask: 1 << index, value: 1 << index }));
    p.push(a("sd-dma-start", Op::Update { reg: sd_reg(index, AZX_REG_SD_CTL, Width::B8), mask: SD_CTL_DMA_START | SD_INT_MASK, value: SD_CTL_DMA_START | SD_INT_MASK }));
    Ok(p)
}

/// `snd_hdac_stream_stop` (stream.c:183-190): clear the DMA and its status, then the stream's interrupt enable.
pub fn stop(streams: &Streams, index: u32) -> Result<Program, HdaError> {
    check(streams, index)?;
    let mut p = Program::new();
    clear_steps(&mut p, index);
    p.push(a("intctl-sie-off", Op::Update { reg: hda(AZX_REG_INTCTL, Width::D32), mask: 1 << index, value: 0 }));
    Ok(p)
}

/// `snd_hdac_stream_cleanup` (stream.c:339-347): the BDL address and the whole control register zeroed.
pub fn cleanup(streams: &Streams, index: u32) -> Result<Program, HdaError> {
    check(streams, index)?;
    let mut p = Program::new();
    p.push(a("sd-bdlpl-clear", Op::Write { reg: sd_reg(index, AZX_REG_SD_BDLPL, Width::D32), value: 0 }));
    p.push(a("sd-bdlpu-clear", Op::Write { reg: sd_reg(index, AZX_REG_SD_BDLPU, Width::D32), value: 0 }));
    p.push(a("sd-ctl-clear", Op::Write { reg: sd_reg(index, AZX_REG_SD_CTL, Width::D32), value: 0 }));
    Ok(p)
}

/// The whole front-end prepare of a playback host stream in Linux's order (pcm.c:759-785): decouple, reset, then (the BDL having been
/// written to memory by the caller from [`crate::bdl::plan_bdl`]) setup, and - when `spib` is `Some((spb, value))` - the SPIB.
pub fn prepare(streams: &Streams, pp: u32, s: &HostStream, params: &SetupParams, spib_at: Option<(u32, u32)>) -> Result<Vec<Program>, HdaError> {
    let mut v: Vec<Program> = Vec::new();
    v.try_reserve(4).map_err(|_| HdaError::OutOfMemory { what: "stream programs" })?;
    let mut first = decouple(streams, pp, s.index, true)?;
    first.extend(&reset(streams, s.index)?);
    v.push(first);
    v.push(setup(streams, s, params)?);
    if let Some((spb, value)) = spib_at {
        v.push(spib(streams, spb, s.index, value, params.buffer_size)?);
    }
    Ok(v)
}
