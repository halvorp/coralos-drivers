// SPDX-License-Identifier: GPL-2.0-only
//! The code-loader DMA transfer of `cldma.c` as a coroutine: `hda_cldma_transfer` (fill the first chunk), `cldma_memcpy_work`
//! (start the engine, then wait for each completion and refill) and `hda_cldma_fill` (the SPIB ring write).
//!
//! REGISTER FRAME: every offset here is relative to BAR4 of the cAVS controller (`adev->dsp_ba`, core.c:468) - the DSP window,
//! [`crate::effect::Space::Dsp`]. The code loader's stream descriptor sits at `regs::AZX_CL_SD_BASE` (0x80) inside it, so `SPIB` is
//! BAR4 + 0xA8 and `SD_CTL` BAR4 + 0x80; they are NOT offsets into the HDA controller's BAR0 (`Space::Hda`). `tests/cldma.rs`
//! and `tests/programs.rs` pin the absolute offsets as literals.
//!
//! Linux runs the loop in a delayed worker, concurrently with whoever scheduled it; a planner is sequential, so the
//! concurrent waiter's work (polling the ROM status, sending LOAD_LIBRARY) is expressed as the optional `hook` step after the
//! first fill (see [`Transfer::new`]) and as the steps that follow the transfer in the composing program.
use crate::effect::*;
use crate::regs::*;
use crate::runner::{Judge, Program, Verdict};

const ADSPIC: Reg = Reg::dsp32(AVS_ADSP_REG_ADSPIC);
const SD_CTL8: Reg = Reg::dsp8(cl_sd(AZX_REG_SD_CTL));
const SPIB: Reg = Reg::dsp32(cl_sd(AZX_REG_CL_SD_SPIB));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum After {
    FirstFilled,
    Started,
    Refilled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Init,
    ReadingSpib(After),
    Draining(After),
    AwaitIoc,
    Done,
}

/// One chunk's placement computed by `hda_cldma_fill` (cldma.c:60-86).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillPlan {
    /// Up to two copies: the tail of the ring, then the wrapped head. `(dst_offset, src_offset, len)`.
    pub copies: [(u32, u32, u32); 2],
    pub ncopies: usize,
    /// The value `SPIB` is set to (`offset + size`).
    pub spib: u32,
    /// Bytes of the image consumed.
    pub consumed: u32,
}

/// `hda_cldma_fill`: given the SPIB read back, the ring size and the bytes still to send, where do the next bytes go?
///
/// `position` is the image offset of the next byte. Refuses (None) a SPIB beyond the buffer, where Linux's
/// `ss = cl->buffer_size - offset` would wrap to a huge unsigned.
pub fn fill_plan(spib: u32, buffer_size: u32, remaining: u32, position: u32) -> Option<FillPlan> {
    let mut size = remaining.min(buffer_size); // cldma.c:64-67
    let mut offset = spib;
    if offset > buffer_size {
        return None;
    }
    let mut pos = position;
    let mut copies = [(0, 0, 0); 2];
    let mut n = 0;
    let mut consumed = 0;
    if offset + size > buffer_size {
        // cldma.c:70-79: the chunk wraps - first the tail of the ring ...
        let ss = buffer_size - offset;
        if ss > 0 {
            copies[n] = (offset, pos, ss);
            n += 1;
        }
        offset = 0;
        size -= ss;
        pos += ss;
        consumed += ss;
    }
    if size > 0 {
        copies[n] = (offset, pos, size); // cldma.c:81
        n += 1;
    }
    consumed += size;
    Some(FillPlan { copies, ncopies: n, spib: offset + size, consumed })
}

#[derive(Clone, Debug)]
pub struct Transfer {
    judge: Judge,
    q: Program,
    qpos: usize,
    state: State,
    buffer_size: u32,
    image_len: u32,
    remaining: u32,
    start_delay_ms: u32,
    hook: Option<Step>,
    last_value: u32,
    failure: Option<Failure>,
}

impl Transfer {
    /// Transfer `image_len` bytes through a ring of `buffer_size` bytes (`hda_cldma_set_data` + `hda_cldma_transfer`).
    ///
    /// `start_delay_ms` is `msecs_to_jiffies(AVS_CLDMA_START_DELAY_MS)` for library and module loads and 0 for the base
    /// firmware (loader.c:179, :201, :237). `hook`, when given, is one step performed after the first chunk is in the buffer
    /// and before the delay: Linux's caller does it between `hda_cldma_transfer()` returning and the worker starting.
    pub fn new(buffer_size: u32, image_len: u32, start_delay_ms: u32, hook: Option<Step>) -> Self {
        Self {
            judge: Judge::new(),
            q: Program::new(),
            qpos: 0,
            state: State::Init,
            buffer_size,
            image_len,
            remaining: image_len,
            start_delay_ms,
            hook,
            last_value: 0,
            failure: None,
        }
    }

    pub const fn remaining(&self) -> u32 {
        self.remaining
    }

    fn fail(&mut self, step: &'static str, cause: Cause) -> Output {
        let f = Failure { step, cause };
        self.failure = Some(f);
        self.state = State::Done;
        Output::Failed(f)
    }

    fn drain(&mut self, after: After) -> Output {
        if self.qpos < self.q.len() {
            let s = self.q.as_slice()[self.qpos];
            self.qpos += 1;
            return Output::Do(self.judge.ask(s));
        }
        self.q = Program::new();
        self.qpos = 0;
        match after {
            After::FirstFilled => {
                // cldma_memcpy_work -> hda_cldma_start (cldma.c:136-149): interrupts on, DMA run bit on, wait for it.
                let both = SD_INT_MASK | SD_CTL_DMA_START;
                if let Some(h) = self.hook {
                    self.q.push(h);
                }
                if self.start_delay_ms > 0 {
                    self.q.push(Step::new("cldma-start-delay", Op::DelayUs(self.start_delay_ms * 1000), OnFail::Abort));
                }
                self.q.push(Step::new("cldma-irq-on", Op::Update { reg: ADSPIC, mask: AVS_ADSP_ADSPIC_CLDMA, value: AVS_ADSP_ADSPIC_CLDMA }, OnFail::Abort));
                self.q.push(Step::new("cldma-run", Op::Update { reg: SD_CTL8, mask: both, value: both }, OnFail::Abort));
                self.q.push(Step::new(
                    "cldma-running",
                    Op::Poll(PollSpec::new(SD_CTL8, SD_CTL_DMA_START, SD_CTL_DMA_START, AVS_CL_OP_INTERVAL_US, AVS_CL_OP_TIMEOUT_US)),
                    OnFail::Abort,
                ));
                self.state = State::Draining(After::Started);
                self.drain(After::Started)
            }
            After::Refilled => {
                // cldma.c:118-120 "enable CLDMA interrupt"
                self.q.push(Step::new("cldma-irq-on", Op::Update { reg: ADSPIC, mask: AVS_ADSP_ADSPIC_CLDMA, value: AVS_ADSP_ADSPIC_CLDMA }, OnFail::Abort));
                self.state = State::Draining(After::Started);
                self.drain(After::Started)
            }
            After::Started => {
                self.state = State::AwaitIoc;
                let s = Step::new("cldma-ioc", Op::Wait { event: Event::CldmaIoc, timeout_ms: AVS_CL_IOC_TIMEOUT_MS }, OnFail::Abort);
                Output::Do(self.judge.ask(s))
            }
        }
    }

    fn read_spib(&mut self, after: After) -> Output {
        self.state = State::ReadingSpib(after);
        let s = Step::new("cldma-spib-read", Op::Read { reg: SPIB }, OnFail::Abort);
        Output::Do(self.judge.ask(s))
    }

    pub fn next(&mut self, input: Input) -> Output {
        match self.state {
            State::Init => {
                if input != Input::Begin {
                    return self.fail("begin", Cause::Protocol);
                }
                if self.remaining == 0 {
                    self.state = State::Done;
                    return Output::Finished; // hda_cldma_transfer: `if (!cl->remaining) return;`
                }
                self.read_spib(After::FirstFilled)
            }
            State::Done => match self.failure {
                Some(f) => Output::Failed(f),
                None => Output::Finished,
            },
            State::AwaitIoc => {
                self.judge.awaiting = None;
                match input {
                    Input::IocStatus(sd) => {
                        if u32::from(sd) & SD_INT_COMPLETE == 0 {
                            return self.fail("cldma-ioc", Cause::CldmaTransferError { sd_status: sd });
                        }
                        if self.remaining == 0 {
                            self.state = State::Done;
                            return Output::Finished;
                        }
                        self.read_spib(After::Refilled)
                    }
                    Input::EventTimedOut => self.fail("cldma-ioc", Cause::EventTimeout),
                    _ => self.fail("cldma-ioc", Cause::Protocol),
                }
            }
            State::ReadingSpib(after) => {
                if let Input::Value(v) = input {
                    self.last_value = v;
                }
                match self.judge.judge(input) {
                    Some((_, Verdict::Ok)) => {}
                    Some((s, Verdict::Fail(c))) => return self.fail(s.label, c),
                    _ => return self.fail("cldma-spib-read", Cause::Protocol),
                }
                let spib = self.last_value;
                let position = self.image_len - self.remaining;
                let Some(plan) = fill_plan(spib, self.buffer_size, self.remaining, position) else {
                    return self.fail("cldma-spib-read", Cause::SpibBeyondBuffer { spib, buffer_size: self.buffer_size });
                };
                self.remaining -= plan.consumed;
                self.q = Program::new();
                self.qpos = 0;
                for &(dst, src, len) in &plan.copies[..plan.ncopies] {
                    self.q.push(Step::new("cldma-copy", Op::CopyToDma { dst_offset: dst, src_offset: src, len }, OnFail::Abort));
                }
                self.q.push(Step::new("cldma-spib-write", Op::Write { reg: SPIB, value: plan.spib }, OnFail::Abort));
                self.state = State::Draining(after);
                self.drain(after)
            }
            State::Draining(after) => match self.judge.judge(input) {
                Some((_, Verdict::Ok)) => self.drain(after),
                Some((s, Verdict::Again)) => Output::Do(s),
                Some((s, Verdict::Fail(c))) => self.fail(s.label, c),
                None => self.fail("protocol", Cause::Protocol),
            },
        }
    }
}
