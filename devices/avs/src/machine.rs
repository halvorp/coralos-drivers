// SPDX-License-Identifier: GPL-2.0-only
//! The ONE seam between the device's logic and the machine it runs on: registers, DMA memory, the I2C bus, GPIO pads, time and
//! blocking waits, the client's endpoints and the firmware channel. The production backing is [`crate::rt`] (coralos-rt, windows the
//! loader granted); the host tests back it with a model of the controller, the DSP and the codec.
//!
//! Nothing above this trait reaches hardware by any other road, so the register allow-list, the I2C address allow-list and the
//! pad allow-list ([`crate::allow`]) are each enforced in exactly one place per layer, and a test machine sees every access.
use avs_fw_core::effect::{Space, Width};
use coralos_hal::{DmaRefusal, DmaRegion};

/// Why a register access did not happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegFault {
    /// The device holds no window for this space (the loader granted none).
    NoWindow { space: Space },
    /// The access is outside the granted window or misaligned.
    Range { space: Space, off: u32 },
    /// A write outside the window's allow-list (the loader-facing half of the fence; [`crate::allow`] is the in-code half).
    NotAllowed { space: Space, off: u32 },
}

impl RegFault {
    pub const fn name(&self) -> &'static str {
        match self {
            RegFault::NoWindow { .. } => "REG-NO-WINDOW",
            RegFault::Range { .. } => "REG-OUT-OF-RANGE",
            RegFault::NotAllowed { .. } => "REG-WRITE-NOT-ALLOWED",
        }
    }
}

/// Why an I2C transfer did not complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I2cFault {
    /// The target did not acknowledge (an address NAK, or a data NAK).
    Nak,
    /// The controller aborted for another reason; the raw `IC_TX_ABRT_SOURCE`.
    Abort { source: u32 },
    /// The transfer did not finish within its bound.
    Timeout,
    /// The controller is not there or not usable (its identity register did not read as a DesignWare core).
    NoController,
    /// The address is not one the device may talk to ([`crate::allow::I2cAllow`]).
    AddressRefused { addr: u16 },
}

impl I2cFault {
    pub const fn name(&self) -> &'static str {
        match self {
            I2cFault::Nak => "I2C-NAK",
            I2cFault::Abort { .. } => "I2C-ABORT",
            I2cFault::Timeout => "I2C-TIMEOUT",
            I2cFault::NoController => "I2C-NO-CONTROLLER",
            I2cFault::AddressRefused { .. } => "I2C-ADDRESS-REFUSED",
        }
    }
}

/// A GPIO pad's register, named by community (an index into the pad windows the loader granted) and a byte offset in that window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadReg {
    pub community: u8,
    pub off: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadFault {
    NoWindow { community: u8 },
    Range { community: u8, off: u32 },
    NotAllowed { community: u8, off: u32 },
}

impl PadFault {
    pub const fn name(&self) -> &'static str {
        match self {
            PadFault::NoWindow { .. } => "PAD-NO-WINDOW",
            PadFault::Range { .. } => "PAD-OUT-OF-RANGE",
            PadFault::NotAllowed { .. } => "PAD-WRITE-NOT-ALLOWED",
        }
    }
}

/// What the device is willing to be woken by in one [`Machine::wait`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Want {
    /// The cAVS interrupt line (stream periods, DSP replies and notifications, the code loader).
    pub avs_irq: bool,
    /// A jack event from the pad interrupt owner.
    pub jack: bool,
    /// A request frame from the client's session, or the session's death.
    pub requests: bool,
    /// A client's `connect()` on the LISTEN endpoint. Not wanted while a session is open: the device serves one client at a time, and a
    /// second connect waits in the kernel's queue until the first session is released.
    pub accept: bool,
}

impl Want {
    pub const NOTHING: Want = Want { avs_irq: false, jack: false, requests: false, accept: false };
    pub const IRQ_ONLY: Want = Want { avs_irq: true, jack: false, requests: false, accept: false };
}

/// What ended a wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    AvsIrq,
    Jack,
    Request,
    /// A client connected: [`Machine::accept`] takes the session.
    Accept,
    /// The open session's client is gone (a tombstone or a dead peer).
    PeerDead,
    /// The pad interrupt owner's endpoint reports its peer dead: the codec's interrupt can no longer reach the device.
    JackGone,
    /// The deadline passed (or, with no deadline and nothing wanted, there is nothing that could ever wake the task - the machine says so).
    Deadline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recv {
    Empty,
    Frame(usize),
    /// A frame larger than the buffer: the byte count it declared. The device answers MALFORMED and drops it.
    Oversized(u64),
    PeerDead,
    Error(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    RingFull,
    PeerDead,
    Denied,
}

/// The firmware channel's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwRecv {
    Frame(usize),
    /// No answer within the bound.
    Timeout,
    /// The broker's endpoint is not there.
    Unavailable,
}

/// Everything the device needs from its host. Every method is bounded: a blocking one takes its bound as an argument.
pub trait Machine {
    // ---- registers ----
    fn reg_read(&mut self, space: Space, off: u32, width: Width) -> Result<u32, RegFault>;
    fn reg_write(&mut self, space: Space, off: u32, width: Width, value: u32) -> Result<(), RegFault>;
    /// The granted length of a window in bytes, 0 when there is none (bounds the multi-link walk).
    fn window_len(&mut self, space: Space) -> u32;

    // ---- DMA memory (the device addresses come from the grant; the CPU view is the host's) ----
    fn dma_alloc(&mut self, size: usize, align: usize) -> Result<DmaRegion, DmaRefusal>;
    fn dma_write(&mut self, region: &DmaRegion, off: usize, data: &[u8]);
    fn dma_read(&mut self, region: &DmaRegion, off: usize, out: &mut [u8]);

    // ---- the I2C bus the codec is on ----
    fn i2c_write(&mut self, addr: u16, data: &[u8]) -> Result<(), I2cFault>;
    fn i2c_write_read(&mut self, addr: u16, write: &[u8], read: &mut [u8]) -> Result<(), I2cFault>;

    // ---- GPIO pads ----
    fn pad_read(&mut self, reg: PadReg) -> Result<u32, PadFault>;
    fn pad_write(&mut self, reg: PadReg, value: u32) -> Result<(), PadFault>;

    // ---- time and blocking ----
    /// Monotonic milliseconds.
    fn now_ms(&mut self) -> u64;
    /// Block until something wanted happens or `deadline_ms` (absolute) passes; `None` is no bound.
    fn wait(&mut self, deadline_ms: Option<u64>, want: Want) -> Wake;
    /// Re-arm the cAVS interrupt line after its cause was serviced (mandatory for a level line).
    fn rearm_avs_irq(&mut self);
    /// Consume one pending jack event without waiting; true if there was one. The loop looks here BEFORE it serves anything else, so a
    /// removal is handled ahead of the requests queued behind it.
    fn take_jack_event(&mut self) -> bool;
    /// Tell the pad interrupt owner the codec's interrupt line was serviced, so it may deliver the next.
    fn rearm_jack(&mut self);

    // ---- the client: one connect() session at a time ----
    /// Take a pending connect as THE session (its announce is consumed); false if there was none or it was not a whole announce.
    fn accept(&mut self) -> bool;
    /// Give the session's endpoints back to the kernel (SESSION_RELEASE) once the client is gone. The kernel never reclaims a server's side
    /// of a session on its own; without this the pair stays held until the device exits. A no-op without a session.
    fn release_session(&mut self);
    fn recv_request(&mut self, buf: &mut [u8]) -> Recv;
    fn send_to_client(&mut self, frame: &[u8]) -> SendOutcome;

    // ---- the firmware channel: send `req`, wait up to `timeout_ms` for the reply frame ----
    fn fw_exchange(&mut self, req: &[u8], reply: &mut [u8], timeout_ms: u32) -> FwRecv;

    /// The task log.
    fn log(&mut self, line: &str);

    // ---- provided: the host learns what the device learned, so it can narrow what it grants ----
    /// The board record was loaded: the host may now open the windows that depend on it (the pad window writable at exactly one register,
    /// the I2C controller with the board's timing).
    fn board_loaded(&mut self, _board: &avs_wire::board::Board) {}
    /// Discovery finished and the write allow-list exists: the host narrows its BAR0 / BAR4 / configuration write grants to it.
    fn layout_ready(&mut self, _allow: &crate::allow::Allow) {}
}
