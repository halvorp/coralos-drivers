// SPDX-License-Identifier: GPL-2.0-only
//! One DSP session: the DSP is powered ONLY while a stream is running. Starting one is the whole bring-up in Linux's AVS order - the
//! controller out of reset, the code loader's buffer, the base firmware and its libraries, the configuration queries, the headphone path
//! created and bound, the host stream, the codec proven safe, the pipelines paused and run - and then the codec's clock, route and output
//! stages powered up (muted) and unmuted at the client's volume. Stopping one is the reverse, and ends with the codec's SAFE STATE
//! proven by readback and the DSP core off.
//!
//! A SPEAKER session (DECISIONS D17) differs in four places and nowhere else: the amplifiers are brought out of reset, configured and
//! READ BACK before the DSP boots ([`Speakers::begin`]); the topology is the speaker one (SSP0, a peakvol stage, the gain ceiling named to
//! the planner); the output stage is powered up after the pipelines run, silent, and raised to the client's level by the DSP's volume
//! stage under the hard ceiling; and stopping silences the stage, switches the parts off, and puts them back in reset.
//!
//! Why per-stream and not resident: no idle power, no state to go stale between streams, and one path to verify instead of two. The cost
//! is the boot time at every start (the base firmware is 243 KB over the code loader's DMA); the wire says a START may take that long.
use crate::codec::{Codec, CodecFail};
use crate::controller::{dsp_off, Controller};
use crate::firmware::Files;
use crate::fwexec::{perform, run_program_with, ExecFail, FwCtx, Images, RunFail};
use crate::ipcrun::{self, Drive, IpcFail};
use crate::machine::{Machine, Want, Wake};
use crate::pump::{Pump, PumpError};
use crate::rig::Rig;
use crate::speakers::{SpkFail, Speakers};
use crate::stream::{Fifo, RingAcct};
use alloc::vec::Vec;
use avs_boot_core::boot::{Action, Boot, Event};
use avs_boot_core::config::{BootConfig, HostSetup, LibImage, PlaybackSel, StreamParams};
use avs_boot_core::error::BootFailure;
use avs_boot_core::plan::Role;
use avs_boot_core::running::Running;
use avs_fw_core::bdl::{ClDmaLayout, Extent};
use avs_fw_core::effect::{Input as FwInput, Reg, Space, Width};
use avs_fw_core::regs::{AVS_CL_DEFAULT_BUFFER_SIZE, AVS_CL_STREAM_TAG, SKL_MIN_FW_VERSION};
use avs_ipc_core::txn::Correlation;
use avs_topology_core::model::SspBinding;
use avs_topology_core::nhlt::Nhlt;
use avs_topology_core::topology::Topology;
use avs_wire::board::Board;
use avs_wire::msg::Format;
use avs_wire::Output;
use max98373_core::gain;
use coralos_hal::{DmaRefusal, DmaRegion};
use hda_ext_core::bdl::plan_bdl;
use hda_ext_core::regs::AZX_REG_SD_LPIB;
use hda_ext_core::stream::{cleanup, stop, HostStream};

/// The one stream shape: what `da7219-tplg.bin` offers (S16, 48 kHz, stereo). The geometry is the device's: 4 periods of 1024 frames
/// (85 ms), a power of two so the ring and its descriptor list are one page each.
pub const PERIOD_FRAMES: u32 = 1024;
pub const PERIODS: u32 = 4;
pub const FORMAT: Format = Format::S16_48K_STEREO;
pub const FRAME_BYTES: u32 = 4;
pub const PERIOD_BYTES: u32 = PERIOD_FRAMES * FRAME_BYTES;
pub const RING_BYTES: u32 = PERIOD_BYTES * PERIODS;

/// While the codec's long sequences run the stream must still be serviced; they look at the interrupt at most this often.
pub const TICK_MS: u64 = 10;

/// The DMA memory a session uses. Allocated once per device lifetime: a grant lives with the task, so it is reused, never re-requested.
#[derive(Clone, Copy, Debug)]
pub struct Bufs {
    pub cldma: DmaRegion,
    pub cldma_bdl: DmaRegion,
    pub ring: DmaRegion,
    pub ring_bdl: DmaRegion,
}

pub fn alloc_bufs<M: Machine>(m: &mut M) -> Result<Bufs, DmaRefusal> {
    Ok(Bufs {
        cldma: m.dma_alloc(AVS_CL_DEFAULT_BUFFER_SIZE as usize, 4096)?,
        cldma_bdl: m.dma_alloc(4096, 4096)?,
        ring: m.dma_alloc(RING_BYTES as usize, 4096)?,
        ring_bdl: m.dma_alloc(4096, 4096)?,
    })
}

/// A booted DSP with the host stream running.
pub struct Dsp {
    pub pump: Pump,
    pub running: Running,
    pub host: HostStream,
    pub acct: RingAcct,
    pub last_period_ms: u64,
    last_tick_ms: u64,
    /// The codec is still coming up: the ring gets silence, the queue is left alone ([`Mode::Hold`]).
    pub hold: bool,
    /// Which path this session is.
    pub out: Output,
    /// The speaker path's DSP gain as last applied (Q1.31 linear; 0 is silence). Always at or below [`gain::CEILING_LINEAR`].
    pub gain: u32,
}

/// Why a session did not start.
#[derive(Debug)]
pub enum StartFail {
    Boot(BootFailure),
    Chip(RunFail),
    Codec(CodecFail),
    Dma(DmaRefusal),
    Layout(&'static str),
    /// A hardware step the plans asked for could not be performed (a refused register, a bus fault under the pump).
    Exec(ExecFail),
    Pump(PumpError),
    OutOfMemory,
    /// The speaker amplifiers: bring-up, protection, power sequence.
    Speaker(SpkFail),
    /// The speaker path's DSP volume stage could not be set.
    Gain(GainFail),
}

impl StartFail {
    /// The step the status names.
    pub fn step(&self) -> &'static str {
        match self {
            StartFail::Boot(f) => f.step,
            StartFail::Chip(f) => f.step(),
            StartFail::Codec(f) => f.step(),
            StartFail::Dma(_) => "dma-alloc",
            StartFail::Layout(w) => w,
            StartFail::Exec(e) => e.name(),
            StartFail::Pump(e) => e.name(),
            StartFail::OutOfMemory => "out-of-memory",
            StartFail::Speaker(f) => f.step(),
            StartFail::Gain(f) => f.name(),
        }
    }
    /// A number that helps: the firmware's refusal status, the bus abort source.
    pub fn detail(&self) -> u32 {
        match self {
            StartFail::Boot(f) => match &f.reason {
                avs_boot_core::error::Reason::IpcRefused { status } => *status,
                avs_boot_core::error::Reason::Fw(fw) => match fw.cause {
                    avs_fw_core::effect::Cause::IpcRefused { status } => status,
                    avs_fw_core::effect::Cause::PollTimeout { last } | avs_fw_core::effect::Cause::PollBound { last, .. } => last,
                    _ => 0,
                },
                _ => 0,
            },
            StartFail::Codec(c) => c.detail(),
            StartFail::Speaker(f) => f.detail(),
            _ => 0,
        }
    }
    /// The boot stage, when there was one.
    pub fn stage(&self) -> &'static str {
        match self {
            StartFail::Boot(f) => f.stage.name(),
            StartFail::Codec(_) => "codec",
            StartFail::Speaker(_) => "speakers",
            StartFail::Gain(_) => "speaker-gain",
            StartFail::Chip(_) => "controller",
            _ => "setup",
        }
    }
}

fn write_bytes<M: Machine>(rig: &mut Rig<M>, region: &DmaRegion, mut pos: usize, data: &[u8]) {
    // a cyclic buffer: the write wraps at the end of the region
    let mut rest = data;
    while !rest.is_empty() {
        let room = region.len - pos;
        let n = rest.len().min(room);
        rig.m.dma_write(region, pos, &rest[..n]);
        rest = &rest[n..];
        pos = (pos + n) % region.len;
    }
}

/// Why the ring is being filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Before the DMA first runs: the whole ring, with SILENCE. The codec is not up yet; real samples played now would be lost to a muted
    /// output, so the client's first samples wait in the queue for the unmute.
    Prime,
    /// While the codec comes up: silence, kept one period ahead so the first samples follow the unmute closely. Not an underrun.
    Hold,
    /// Playing: queued PCM first, silence only to keep one period ahead, and that silence IS an underrun.
    Run,
}

/// Move queued PCM (and, if the hardware would otherwise starve, silence) into the ring.
pub fn refill<M: Machine>(rig: &mut Rig<M>, ring: &DmaRegion, acct: &mut RingAcct, fifo: &mut Fifo, mode: Mode) {
    let (queued, margin) = match mode {
        Mode::Prime => (0, acct.ring_bytes as usize),
        Mode::Hold => (0, acct.period_bytes as usize),
        Mode::Run => (fifo.queued(), acct.period_bytes as usize),
    };
    let fill = acct.plan_fill(queued, margin);
    let mut chunk = [0u8; 1024];
    let mut left = fill.pcm;
    let mut pos = acct.write_pos();
    while left > 0 {
        let n = left.min(chunk.len());
        let got = fifo.pop_into(&mut chunk[..n], acct.frame_bytes as usize);
        if got == 0 {
            break;
        }
        write_bytes(rig, ring, pos, &chunk[..got]);
        pos = (pos + got) % ring.len;
        left -= got;
    }
    let mut sil = fill.silence;
    chunk.fill(0);
    while sil > 0 {
        let n = sil.min(chunk.len());
        write_bytes(rig, ring, pos, &chunk[..n]);
        pos = (pos + n) % ring.len;
        sil -= n;
    }
    acct.commit(fill, mode == Mode::Run);
}

fn lpib(index: u32) -> Reg {
    Reg { space: Space::Hda, off: hda_ext_core::consts::SD_BASE + hda_ext_core::consts::SD_INTERVAL * index + AZX_REG_SD_LPIB, width: Width::D32 }
}

impl Dsp {
    /// Fold the hardware position in and refill. Called for each serviced period interrupt.
    pub fn on_periods<M: Machine>(&mut self, rig: &mut Rig<M>, bufs: &Bufs, fifo: &mut Fifo) -> Result<(), PumpError> {
        let pos = rig.read(lpib(self.host.index))?;
        if !self.acct.advance_hw(pos) {
            return Err(PumpError::StreamError { sts: 0xff });
        }
        refill(rig, &bufs.ring, &mut self.acct, fifo, if self.hold { Mode::Hold } else { Mode::Run });
        self.last_period_ms = rig.now();
        Ok(())
    }

    /// Service one firing of the cAVS line: the pump, then the ring if a period completed.
    pub fn service<M: Machine>(&mut self, rig: &mut Rig<M>, bufs: &Bufs, fifo: &mut Fifo) -> Result<bool, PumpError> {
        self.pump.service_irq(rig)?;
        rig.m.rearm_avs_irq();
        if self.pump.pend.periods > 0 {
            self.pump.pend.periods = 0;
            self.on_periods(rig, bufs, fifo)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// During the codec's long sequences: look at the interrupt (without waiting for it) every [`TICK_MS`].
    pub fn tick<M: Machine>(&mut self, rig: &mut Rig<M>, bufs: &Bufs, fifo: &mut Fifo) -> Result<(), PumpError> {
        let now = rig.now();
        if now.saturating_sub(self.last_tick_ms) < TICK_MS {
            return Ok(());
        }
        self.last_tick_ms = now;
        if let Wake::AvsIrq = rig.wait(Some(now), Want::IRQ_ONLY) {
            self.service(rig, bufs, fifo)?;
        } else if self.pump.pend.periods > 0 {
            // periods the pump counted while it waited for something else (an IPC reply) are still owed their refill
            self.pump.pend.periods = 0;
            self.on_periods(rig, bufs, fifo)?;
        }
        Ok(())
    }
}

fn widget_names(ssp: u8) -> ([u8; 12], [u8; 12], usize) {
    // the AVS topologies name the headset's playback widgets `ssp<N>p_fe` and `ssp<N>p_be`
    let mut fe = [0u8; 12];
    let mut be = [0u8; 12];
    let s = [b's', b's', b'p', b'0' + ssp.min(9), b'p', b'_'];
    fe[..6].copy_from_slice(&s);
    be[..6].copy_from_slice(&s);
    fe[6..8].copy_from_slice(b"fe");
    be[6..8].copy_from_slice(b"be");
    (fe, be, 8)
}

/// What a session start works from, borrowed from the device's fields.
pub struct StartArgs<'a> {
    pub ctl: &'a Controller,
    pub files: &'a Files,
    pub bufs: &'a Bufs,
    pub board: &'a Board,
    /// The volume to come up at (`Some(0)` is muted); `None` is the conservative initial gain.
    pub level: Option<u16>,
    /// Which path to bring up. Never [`Output::None`].
    pub out: Output,
    /// The speaker topology file (`max98373-tplg.bin`); required for [`Output::Speakers`].
    pub speaker_topology: Option<&'a [u8]>,
}

/// The level a speaker stream plays at before its client has chosen one: quiet (level 100 is 42.75 dB under the ceiling).
pub const SPEAKER_INITIAL_LEVEL: u16 = 100;

/// Why the speaker gain could not be set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GainFail {
    /// The path has no volume stage (the planner refuses such a speaker path; defensive).
    NoVolumeStage,
    /// A request above the ceiling reached the handle: it was refused there. (The mapping never produces one.)
    AboveCeiling,
    Ipc(ipcrun::IpcFail),
}

impl GainFail {
    pub const fn name(&self) -> &'static str {
        match self {
            GainFail::NoVolumeStage => "speaker-no-volume-stage",
            GainFail::AboveCeiling => "speaker-gain-above-ceiling",
            GainFail::Ipc(_) => "speaker-gain-ipc",
        }
    }
}

/// The DSP gain a client's level means on the speaker path: the ONLY place a speaker gain is derived. `None` is the initial level.
pub fn speaker_gain_for(level: Option<u16>, mute: bool) -> u32 {
    gain::linear_for(level.unwrap_or(SPEAKER_INITIAL_LEVEL), mute)
}

/// Set the speaker path's DSP volume. The number is clamped to the ceiling here, and the handle refuses anything above its own ceiling a
/// second time, so there are two checks between any caller and the module.
pub fn set_speaker_gain<M: Machine>(rig: &mut Rig<M>, dsp: &mut Dsp, linear: u32) -> Result<(), GainFail> {
    let linear = gain::clamp(linear);
    let handle = dsp.running.peakvols.first().copied().ok_or(GainFail::NoVolumeStage)?;
    let msgs = handle.set_volume(linear).map_err(|_| GainFail::AboveCeiling)?;
    for m in &msgs {
        let req = m.as_request();
        ipcrun::begin(rig, &mut dsp.pump, &req, m.expected_reply, false).map_err(GainFail::Ipc)?;
    }
    dsp.gain = linear;
    Ok(())
}

/// Bring the DSP up and start the stream. On any failure everything that was started is taken down again and the output stage is left safe.
pub fn start_dsp<M: Machine>(rig: &mut Rig<M>, a: &StartArgs<'_>, codec: &mut Codec, speakers: Option<&mut Speakers>, fifo: &mut Fifo) -> Result<Dsp, StartFail> {
    let ctl = a.ctl;
    let mut speakers = speakers;
    if a.out == Output::Speakers {
        // the amplifiers first: configured, protected and read back before a DSP exists to feed them. Nothing is enabled.
        let Some(s) = speakers.as_deref_mut() else { return Err(StartFail::Layout("no-speakers")) };
        if let Err(e) = s.begin(rig, &mut |_| Ok(())) {
            let _ = s.safe(rig, true);
            return Err(StartFail::Speaker(e));
        }
    }
    if let Err(e) = ctl.start_chip(rig) {
        if let Some(s) = speakers.as_deref_mut() {
            let _ = s.safe(rig, true);
        }
        return Err(StartFail::Chip(e));
    }
    let r = start_inner(rig, a, codec, speakers.as_deref_mut(), fifo);
    if r.is_err() {
        // whatever stage failed: the DSP core off, the controller down, the output stage safe. Errors from the clean-up are secondary.
        let _ = dsp_off(rig);
        let _ = ctl.stop_chip(rig);
        match a.out {
            Output::Speakers => {
                if let Some(s) = speakers.as_deref_mut() {
                    let _ = s.safe(rig, true);
                }
            }
            _ => {
                let _ = codec.safe_state(rig);
            }
        }
    }
    r
}

fn start_inner<M: Machine>(rig: &mut Rig<M>, a: &StartArgs<'_>, codec: &mut Codec, mut speakers: Option<&mut Speakers>, fifo: &mut Fifo) -> Result<Dsp, StartFail> {
    let StartArgs { ctl, files, bufs, board, level, out, speaker_topology } = *a;
    let speaker_path = out == Output::Speakers;
    // ---- the code loader's buffer and descriptor list ----
    let layout = ClDmaLayout::plan(&[Extent { addr: bufs.cldma.phys, len: AVS_CL_DEFAULT_BUFFER_SIZE }], bufs.cldma_bdl.phys, AVS_CL_DEFAULT_BUFFER_SIZE, AVS_CL_STREAM_TAG).map_err(|e| StartFail::Layout(e.name()))?;
    for (i, e) in layout.entries().iter().enumerate() {
        rig.m.dma_write(&bufs.cldma_bdl, i * 16, &e.to_le_bytes());
    }
    // ---- the host stream's ring: primed, and its descriptor list planned (Boot writes the list when asked) ----
    let ring_bdl = plan_bdl(&[Extent { addr: bufs.ring.phys, len: RING_BYTES }], RING_BYTES, PERIOD_BYTES, true).map_err(|e| StartFail::Layout(e.name()))?;
    let mut acct = RingAcct::new(RING_BYTES, PERIOD_BYTES, FRAME_BYTES);
    refill(rig, &bufs.ring, &mut acct, fifo, Mode::Prime);
    // ---- what Boot is given ----
    // the speaker card is a machine with ONE SSP and no TDM split: `%d` in the topology's names becomes the port, and its copier's I2S gateway
    // gets its virtual index (avs-topology-core `SspBinding`, topology.c:398-422)
    let speaker_ssp_port = board.speaker_ssps().first().copied().unwrap_or(0);
    let (topology, speaker_path_libs) = if speaker_path {
        let bytes = speaker_topology.ok_or(StartFail::Layout("no-speaker-topology"))?;
        let t = Topology::parse(bytes, Some(SspBinding { port: speaker_ssp_port as u32, tdm: 0 })).map_err(|_| StartFail::Layout("speaker-topology-parse"))?;
        (t, true)
    } else {
        (Topology::parse(&files.topology, None).map_err(|_| StartFail::Layout("topology-parse"))?, false)
    };
    let nhlt = Nhlt::parse(&files.nhlt, true).map_err(|_| StartFail::Layout("nhlt-parse"))?;
    let mut libs: Vec<LibImage> = Vec::new();
    if speaker_path_libs {
        // the speaker topology's libraries would be fetched with it; the upstream one names none, and one that does is refused rather than run without
        if !topology.libs.is_empty() {
            return Err(StartFail::Layout("speaker-topology-needs-libraries"));
        }
    } else {
        libs.try_reserve_exact(files.libs.len()).map_err(|_| StartFail::OutOfMemory)?;
        libs.extend(files.libs.iter().map(|l| LibImage { name: l.name, image_len: (l.bytes.len() - l.body_at) as u32 }));
    }
    let mut speaker_ssp: Vec<u32> = Vec::new();
    speaker_ssp.try_reserve_exact(board.n_speaker_ssp as usize).map_err(|_| StartFail::OutOfMemory)?;
    speaker_ssp.extend(board.speaker_ssps().iter().map(|s| *s as u32));
    let (fe, be, wn) = widget_names(if speaker_path { speaker_ssp_port } else { board.headset_ssp });
    let params = StreamParams { rate: FORMAT.rate, channels: FORMAT.channels as u32, container_bits: FORMAT.container_bits as u32, valid_bits: FORMAT.valid_bits as u32 };
    let cfg = BootConfig {
        topology: &topology,
        nhlt: &nhlt,
        layout,
        basefw_len: files.base_body().len() as u32,
        libs,
        playback: PlaybackSel { fe_widget: &fe[..wn], be_widget: &be[..wn], fe: params, be: params, ssp_instance: if speaker_path { speaker_ssp_port as u32 } else { board.headset_ssp as u32 }, speaker_ssp: &speaker_ssp, speaker_ceiling: if speaker_path { Some(gain::CEILING_LINEAR) } else { None } },
        host: HostSetup {
            streams: ctl.streams,
            playback_stream: board.playback_stream as u32,
            pp: ctl.caps.pp.ok_or(StartFail::Layout("no-processing-pipe"))?,
            spb: ctl.caps.spb,
            bdl: ring_bdl,
            bdl_addr: bufs.ring_bdl.phys,
            posbuf: false,
            spib: ctl.caps.spb.map(|_| RING_BYTES),
            format_bits: FORMAT.container_bits as u32,
        },
        min_fw: SKL_MIN_FW_VERSION,
    };
    let host = ctl.streams.playback_stream(board.playback_stream as u32).map_err(|e| StartFail::Layout(e.name()))?;
    let mut boot = Boot::new(&cfg).map_err(StartFail::Boot)?;
    let mut pump = Pump::new(Correlation::Strict);
    // the stream's interrupt is serviced from the moment the DMA can run
    pump.stream = Some(host.index);
    let mut libs_body: Vec<&[u8]> = Vec::new();
    libs_body.try_reserve_exact(files.libs.len()).map_err(|_| StartFail::OutOfMemory)?;
    libs_body.extend(files.libs.iter().map(|l| &l.bytes[l.body_at..]));
    let images = Images { base: files.base_body(), libs: libs_body };
    let mut ctx = FwCtx::loader(&images, bufs.cldma);

    // ---- drive the orchestrator ----
    let mut action = boot.next(Event::Begin);
    let running = 'drive: {
        for _ in 0..50_000u32 {
            let ev = match action {
                Action::Fw(step) => {
                    ctx.current = boot.image().or(ctx.current);
                    match perform(rig, &mut pump, &mut ctx, &step) {
                        Ok(i) => Event::Fw(i),
                        Err(e) => break 'drive Err(StartFail::Exec(e)),
                    }
                }
                Action::Ipc(msg) => {
                    let req = msg.as_request();
                    match ipcrun::begin(rig, &mut pump, &req, msg.expected_reply, false) {
                        Ok(Drive::Done(c)) => {
                            let n = c.payload_len.min(pump.reply.len());
                            action = boot.next(Event::IpcReply { header: c.header, payload: &pump.reply[..n] });
                            continue;
                        }
                        Ok(Drive::Parked(_)) => Event::IpcError(avs_ipc_core::error::IpcError::Protocol),
                        Err(IpcFail::Ipc(e)) => Event::IpcError(e),
                        Err(IpcFail::Pump(p)) => break 'drive Err(StartFail::Pump(p)),
                    }
                }
                Action::WriteBdl(bytes) => {
                    rig.m.dma_write(&bufs.ring_bdl, 0, &bytes);
                    Event::BdlWritten
                }
                Action::AskCodecSafe => Event::CodecSafe(match speakers.as_deref_mut() {
                    // before any I2S clock starts: neither amplifier is enabled or receiving (or both are in reset), and the codec is safe too
                    Some(s) if speaker_path => s.is_idle(rig) && codec.is_safe(rig),
                    _ => codec.safe_state(rig).is_ok(),
                }),
                Action::Finished(r) => break 'drive Ok(r),
                Action::Failed(f) => break 'drive Err(StartFail::Boot(f)),
            };
            action = boot.next(ev);
        }
        Err(StartFail::Layout("boot-never-ended"))
    }?;
    let _ = FwInput::Done;
    let mut dsp = Dsp { pump, running, host, acct, last_period_ms: rig.now(), last_tick_ms: rig.now(), hold: true, out, gain: 0 };

    // ---- the output stage: clocks, route, output stages, volume ----
    if speaker_path {
        let Some(s) = speakers.as_deref_mut() else { return Err(StartFail::Layout("no-speakers")) };
        speakers_up(rig, &mut dsp, bufs, fifo, s, level)?;
    } else {
        codec_up(rig, &mut dsp, bufs, fifo, codec, level)?;
    }
    Ok(dsp)
}

fn codec_up<M: Machine>(rig: &mut Rig<M>, dsp: &mut Dsp, bufs: &Bufs, fifo: &mut Fifo, codec: &mut Codec, level: Option<u16>) -> Result<(), StartFail> {
    let (rate, bits, ch) = (FORMAT.rate, FORMAT.container_bits as u32, FORMAT.channels as u32);
    codec.power_up(rig, rate, bits, ch, &mut |rig| dsp.tick(rig, bufs, fifo)).map_err(StartFail::Codec)?;
    dsp.tick(rig, bufs, fifo).map_err(StartFail::Pump)?;
    codec.unmute(rig, level, &mut |rig| dsp.tick(rig, bufs, fifo)).map_err(StartFail::Codec)?;
    // the output is live: from the next period on, the client's samples go in (and an empty queue is an underrun)
    dsp.hold = false;
    dsp.on_periods(rig, bufs, fifo).map_err(StartFail::Pump)?;
    Ok(())
}

/// The speaker path's last step: the parts enabled (the pipelines run, the clocks exist, the protection was read back), the DSP volume stage
/// raised from silence to the client's level under the ceiling, and the client's samples let in.
fn speakers_up<M: Machine>(rig: &mut Rig<M>, dsp: &mut Dsp, bufs: &Bufs, fifo: &mut Fifo, speakers: &mut Speakers, level: Option<u16>) -> Result<(), StartFail> {
    speakers.power_up(rig, &mut |rig| dsp.tick(rig, bufs, fifo)).map_err(StartFail::Speaker)?;
    dsp.tick(rig, bufs, fifo).map_err(StartFail::Pump)?;
    set_speaker_gain(rig, dsp, speaker_gain_for(level, false)).map_err(StartFail::Gain)?;
    dsp.hold = false;
    dsp.on_periods(rig, bufs, fifo).map_err(StartFail::Pump)?;
    Ok(())
}

/// What a stop managed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StopReport {
    /// The output stage is in its verified safe state: the codec by readback (headphones), the amplifiers in hardware reset by the pad's
    /// readback (speakers); and the stage that was not in use still is.
    pub codec_safe: bool,
    /// The DSP core was switched off and the controller stopped.
    pub dsp_off: bool,
    /// Steps of the graceful path that failed (ignored: the next step still ran).
    pub errors: u32,
    /// For a speaker session: what the amplifiers' reset line is after the stop.
    pub spk_hold: Option<avs_wire::SpeakerHold>,
}

/// Stop the stream and take everything down. `graceful`: also pause the pipelines through the DSP (skipped when the DSP is not to be
/// trusted - after an exception or a time-out - so a dead DSP does not cost a time-out per message).
pub fn stop_dsp<M: Machine>(rig: &mut Rig<M>, ctl: &Controller, codec: &mut Codec, speakers: Option<&mut Speakers>, mut dsp: Dsp, graceful: bool) -> StopReport {
    let mut errors = 0;
    // 1. the output stage first: the cheapest thing to hear, and the one safety property that matters
    let mut rep = if dsp.out == Output::Speakers {
        // silence in the DSP (when it can still answer), the parts off and proven, the reset low and proven; then the codec is checked too
        if graceful && dsp.pump.pend.exception.is_none() && set_speaker_gain(rig, &mut dsp, 0).is_err() {
            errors += 1;
        }
        let spk = match speakers {
            Some(s) => s.safe(rig, true),
            None => crate::speakers::SpkSafe { amps_off: false, held: avs_wire::SpeakerHold::NotGranted },
        };
        StopReport { codec_safe: spk.verified() && codec.is_safe(rig), spk_hold: Some(spk.held), ..StopReport::default() }
    } else {
        let mut r = StopReport { codec_safe: codec.power_down(rig).is_ok() && codec.is_safe(rig), ..StopReport::default() };
        if let Some(s) = speakers {
            // the amplifiers were in reset all along; prove it again, cheaply, without writing
            r.codec_safe = r.codec_safe && crate::gpio::speakers_held(rig, s.pad());
        }
        r
    };
    rep.errors = errors;
    // 2. the host stream
    for p in [stop(&ctl.streams, dsp.host.index), cleanup(&ctl.streams, dsp.host.index)] {
        match p {
            Ok(p) => {
                if run_program_with(rig, &mut dsp.pump, &p).is_err() {
                    rep.errors += 1;
                }
            }
            Err(_) => rep.errors += 1,
        }
    }
    // 3. the pipelines, front end first then back end (pcm.c:895-908), only if the DSP can still answer
    if graceful && dsp.pump.pend.exception.is_none() {
        for role in [Role::FrontEnd, Role::BackEnd] {
            if let Ok(msgs) = dsp.running.pause_msgs(role) {
                for m in &msgs {
                    let req = m.as_request();
                    if ipcrun::begin(rig, &mut dsp.pump, &req, m.expected_reply, false).is_err() {
                        rep.errors += 1;
                        break;
                    }
                }
            }
        }
    }
    // 4. the DSP core off and the gating back on, then the controller
    let off = dsp_off(rig).is_ok();
    let chip = ctl.stop_chip(rig).is_ok();
    rep.dsp_off = off && chip;
    // the output stage is checked once more AFTER the clocks are gone: stopping the I2S clocks must not have un-safed it
    rep.codec_safe = rep.codec_safe && codec.is_safe(rig);
    rep
}
