// SPDX-License-Identifier: GPL-2.0-only
//! The Kaby Lake base-firmware boot as one plan: `avs_cldma_load_basefw` (loader.c:138-192) plus the FW_READY wait of
//! `avs_dsp_load_basefw` (loader.c:589-598), and the library and module loads that reuse the same transfer
//! (loader.c:194-252).
//!
//! NOT in this plan, deliberately, because Linux does them around it: the gating bracket ([`programs::gating`]), the
//! first-boot `avs_dsp_core_disable` ([`programs::core_disable`]), reading the image and checking its envelope
//! ([`crate::manifest::strip_verify`]), writing the BDL entries to the BDL page, and `avs_module_info_init` after the
//! libraries (an IPC, in avs-ipc-core). The composition is the `.device`'s.
use crate::bdl::ClDmaLayout;
use crate::cldma::Transfer;
use crate::effect::*;
use crate::error::FwError;
use crate::programs;
use crate::regs::*;
use crate::runner::{Program, Runner};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Pre,
    Xfer,
    Post,
    Abort,
    Done,
}

#[derive(Clone, Debug)]
pub struct BaseFwBoot {
    stage: Stage,
    pre: Runner,
    xfer: Transfer,
    post: Runner,
    abort: Option<Runner>,
}

impl BaseFwBoot {
    /// `image_len` is the length of the STRIPPED image ([`crate::manifest::Stripped::body`]) - the bytes the DMA carries.
    pub fn new(layout: &ClDmaLayout, image_len: u32) -> Result<Self, FwError> {
        if image_len == 0 {
            return Err(FwError::ImageTooShort { have: 0, need: 1 });
        }
        Ok(Self {
            stage: Stage::Pre,
            pre: Runner::new(programs::basefw_pre(layout), Program::new())?,
            xfer: Transfer::new(layout.buffer_size, image_len, 0, None),
            post: Runner::new(programs::basefw_post(), Program::new())?,
            abort: None,
        })
    }

    pub fn next(&mut self, input: Input) -> Output {
        let mut input = input;
        loop {
            match self.stage {
                Stage::Pre => match self.pre.next(input) {
                    Output::Finished => {
                        self.stage = Stage::Xfer;
                        input = Input::Begin;
                    }
                    o => return o,
                },
                Stage::Xfer => match self.xfer.next(input) {
                    Output::Finished => {
                        self.stage = Stage::Post;
                        input = Input::Begin;
                    }
                    Output::Failed(f) => {
                        // loader.c:183-188: stop the loader, disable the core, return the error.
                        match Runner::aborting(f, OnFail::AbortStopDisable, Program::new()) {
                            Ok(r) => {
                                self.abort = Some(r);
                                self.stage = Stage::Abort;
                                input = Input::Begin;
                            }
                            Err(_) => return Output::Failed(f),
                        }
                    }
                    o => return o,
                },
                Stage::Post => match self.post.next(input) {
                    Output::Finished => {
                        self.stage = Stage::Done;
                        return Output::Finished;
                    }
                    o => return o,
                },
                Stage::Abort => {
                    return match self.abort.as_mut() {
                        Some(r) => r.next(input),
                        None => Output::Finished,
                    }
                }
                Stage::Done => return Output::Finished,
            }
        }
    }
}

/// `avs_cldma_load_library` (loader.c:194-213) and `avs_cldma_load_module` (loader.c:215-252): fill the first chunk, send
/// LOAD_LIBRARY / LOAD_MULTIPLE_MODULES, wait 100 ms, run the DMA, await the DSP's reply, stop the loader (result ignored).
/// A failure is the image's alone: Linux does not disable the core. The module path also brackets itself with
/// [`programs::gating`]; the `.device` composes that.
#[derive(Clone, Debug)]
pub struct CodeLoad {
    xfer: Transfer,
    tail: Runner,
    in_tail: bool,
    failed: Option<Failure>,
}

impl CodeLoad {
    /// Load a library image (`image_len` = its stripped length) as `lib_id`.
    pub fn library(layout: &ClDmaLayout, image_len: u32, lib_id: u32) -> Result<Self, FwError> {
        Self::new(layout, image_len, IpcCall::LoadLibrary { lib_id })
    }

    /// Load the module image `dsp_mod_<uuid>.bin` as module `module_id`.
    pub fn module(layout: &ClDmaLayout, image_len: u32, module_id: u16) -> Result<Self, FwError> {
        Self::new(layout, image_len, IpcCall::LoadModule { module_id })
    }

    fn new(layout: &ClDmaLayout, image_len: u32, call: IpcCall) -> Result<Self, FwError> {
        if image_len == 0 {
            return Err(FwError::ImageTooShort { have: 0, need: 1 });
        }
        let send = Step::new("code-ipc-send", Op::IpcSend(call), OnFail::Abort);
        let mut tail = Program::new();
        tail.push(Step::new("code-ipc-reply", Op::IpcAwait { timeout_ms: AVS_CL_IPC_TIMEOUT_MS }, OnFail::Abort));
        let mut fin = Program::new();
        fin.extend(&programs::cldma_stop(OnFail::Tolerate)); // loader.c:205
        Ok(Self { xfer: Transfer::new(layout.buffer_size, image_len, AVS_CLDMA_START_DELAY_MS, Some(send)), tail: Runner::new(tail, fin)?, in_tail: false, failed: None })
    }

    pub fn next(&mut self, input: Input) -> Output {
        let mut input = input;
        loop {
            match self.in_tail {
                false => match self.xfer.next(input) {
                    Output::Finished => {
                        self.in_tail = true;
                        input = Input::Begin;
                    }
                    Output::Failed(f) => {
                        // the stop still runs; the failure is reported after it
                        self.failed = Some(f);
                        match Runner::new(Program::new(), programs::cldma_stop(OnFail::Tolerate)) {
                            Ok(r) => {
                                self.tail = r;
                                self.in_tail = true;
                                input = Input::Begin;
                            }
                            Err(_) => return Output::Failed(f),
                        }
                    }
                    o => return o,
                },
                true => {
                    return match self.tail.next(input) {
                        Output::Finished => match self.failed {
                            Some(f) => Output::Failed(f),
                            None => Output::Finished,
                        },
                        o => o,
                    }
                }
            }
        }
    }
}

/// The bound on a LOAD_LIBRARY / LOAD_MODULES reply: `AVS_CL_TIMEOUT_MS` (messages.c:13).
pub const AVS_CL_IPC_TIMEOUT_MS: u32 = 5000; // messages.c:13
