// SPDX-License-Identifier: GPL-2.0-only
//! The simulated Atlas: one `Machine` that is a controller, a DSP, a codec on an I2C bus, a pad controller, DMA memory, a firmware
//! broker and a client, all on a VIRTUAL clock. Time moves only when the device waits: a wait runs the stream engines and the scheduled
//! events up to the next thing that could wake it, so a test sees exactly the interleaving the hardware would produce, deterministically.
//!
//! Every register write, I2C transaction and pad write is RECORDED, and the window grants a real loader would give (read-only until the
//! device narrows them from its own lists) are enforced here too, so a test can assert what was touched and what never was.
#![allow(dead_code)]
pub mod amp;
pub mod codec;
pub mod defaults;
pub mod dsp;
pub mod harness;
pub mod hda;
pub mod ucm_image;
pub mod world;

use avs_device::machine::*;
use avs_fw_core::effect::{Space, Width};
use avs_wire::fw::{read_reply, stat_reply, FwRead, FwStat};
use avs_wire::{Header, Opcode, Status};
use coralos_hal::{DmaRefusal, DmaRegion};
use std::collections::{HashMap, VecDeque};

pub enum Ev {
    Request(Vec<u8>),
    Plug { four_pole: bool },
    Unplug,
    /// Headset button `n` goes down.
    Press { button: u8 },
    Release { button: u8 },
    PeerDead,
    /// The pad interrupt owner dies.
    JackOwnerDead,
    Custom(Box<dyn FnOnce(&mut Sim)>),
}

pub struct Region {
    pub phys: u64,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct WaitRec {
    pub at_us: u64,
    pub deadline_ms: Option<u64>,
    pub want: Want,
    pub woke: Wake,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I2cRec {
    pub addr: u16,
    pub write: Vec<u8>,
    pub read_len: usize,
    pub acked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteRec {
    pub space: Space,
    pub off: u32,
    pub width: Width,
    pub value: u32,
}

pub struct Sim {
    pub t_us: u64,
    evs: Vec<(u64, u64, Ev)>,
    seq: u64,
    pub hda: hda::Hda,
    pub dsp: dsp::Dsp,
    pub codec: codec::Codec,
    /// The speaker amplifiers: the right one (0x31) then the left (0x32).
    pub amps: [amp::Amp; 2],
    /// A global sequence number, bumped on every register write, I2C transaction and pad write, so a test can order them.
    pub tx_seq: u64,
    /// The sequence number of the first register write after which the playback stream's DMA engine was running.
    pub first_dma_seq: Option<u64>,
    pub cfg: [u8; 256],
    pub pad2: Vec<u8>,
    /// The write grants a real loader would have narrowed to the device's lists (None: still read-only).
    pub grant_hda: Option<Vec<(u32, u32)>>,
    pub grant_dsp: Option<Vec<(u32, u32)>>,
    pub grant_cfg: Option<Vec<(u32, u32)>>,
    pub pad_granted: bool,
    pub regions: Vec<Region>,
    next_phys: u64,
    pub dma_budget: Option<usize>,
    dma_used: usize,
    pub writes: Vec<WriteRec>,
    pub refused_writes: Vec<WriteRec>,
    pub i2c: Vec<I2cRec>,
    pub pad_writes: Vec<(u32, u32)>,
    /// The virtual time (us) of each pad write, aligned with `pad_writes`.
    pub pad_times_us: Vec<u64>,
    pub inbox: VecDeque<Vec<u8>>,
    pub outbox: Vec<Vec<u8>>,
    pub client_dead: bool,
    pub jack_owner_dead: bool,
    /// Clients that have connect()ed and not been accepted; whether a session is open; how many sessions were handed back.
    pub listen_pending: u32,
    pub session_open: bool,
    pub released: u32,
    pub ring_full: u32,
    jack_msgs: u32,
    jack_armed: bool,
    jack_prev_line: bool,
    pub jack_acks: u32,
    pub files: HashMap<String, Vec<u8>>,
    pub fw_requests: Vec<String>,
    pub broker_down: bool,
    pub broker_timeout: bool,
    pub waits: Vec<WaitRec>,
    pub blocked_forever: u32,
    pub logs: Vec<String>,
    pub irq_rearms: u32,
    pub board_loaded: bool,
    pub layout_ready_calls: u32,
    pub i2c_cost_us: u64,
    /// The cAVS interrupt line is stuck asserted (a cause nobody can clear).
    pub force_irq: bool,
    /// The speaker-reset pad reads high whatever is driven (something else holds the amplifiers out of reset).
    pub pad_stuck_high: bool,
    /// The speaker-reset pad reads low whatever is driven (the amplifiers can never be released from reset).
    pub pad_stuck_low: bool,
}

pub fn read_region(regions: &[Region], addr: u64, len: usize) -> Vec<u8> {
    for r in regions {
        if addr >= r.phys && addr + len as u64 <= r.phys + r.data.len() as u64 {
            let o = (addr - r.phys) as usize;
            return r.data[o..o + len].to_vec();
        }
    }
    vec![0; len]
}

impl Sim {
    pub fn new(fw: dsp::Firmware, basefw_len: usize) -> Self {
        let mut cfg = [0u8; 256];
        cfg[0..4].copy_from_slice(&0x9d71_8086u32.to_le_bytes());
        let mut pad2 = vec![0u8; 0x1000];
        pad2[0x0c..0x10].copy_from_slice(&0x400u32.to_le_bytes());
        // every pad reads high (pulled up) until something drives it
        for p in 0..24usize {
            let o = 0x400 + p * 8;
            pad2[o..o + 4].copy_from_slice(&0x0000_0002u32.to_le_bytes());
        }
        Self {
            t_us: 1_000_000,
            evs: vec![],
            seq: 0,
            hda: hda::Hda::new(),
            dsp: dsp::Dsp::new(fw, basefw_len),
            codec: codec::Codec::new(),
            amps: [amp::Amp::new(0x31), amp::Amp::new(0x32)],
            tx_seq: 0,
            first_dma_seq: None,
            cfg,
            pad2,
            grant_hda: None,
            grant_dsp: None,
            grant_cfg: None,
            pad_granted: true,
            regions: vec![],
            next_phys: 0x1000_0000,
            dma_budget: None,
            dma_used: 0,
            writes: vec![],
            refused_writes: vec![],
            i2c: vec![],
            pad_writes: vec![],
            pad_times_us: vec![],
            inbox: VecDeque::new(),
            outbox: vec![],
            client_dead: false,
            jack_owner_dead: false,
            listen_pending: 1,
            session_open: false,
            released: 0,
            ring_full: 0,
            jack_msgs: 0,
            jack_armed: true,
            jack_prev_line: false,
            jack_acks: 0,
            files: HashMap::new(),
            fw_requests: vec![],
            broker_down: false,
            broker_timeout: false,
            waits: vec![],
            blocked_forever: 0,
            logs: vec![],
            irq_rearms: 0,
            board_loaded: false,
            layout_ready_calls: 0,
            i2c_cost_us: 300,
            force_irq: false,
            pad_stuck_high: false,
            pad_stuck_low: false,
        }
    }

    // ---- scheduling ----
    pub fn at_ms(&mut self, ms_from_now: u64, ev: Ev) {
        self.seq += 1;
        let at = self.t_us + ms_from_now * 1000;
        self.evs.push((at, self.seq, ev));
    }

    pub fn request(&mut self, frame: Vec<u8>) {
        self.inbox.push_back(frame);
    }

    pub fn now_ms_f(&self) -> u64 {
        self.t_us / 1000
    }

    /// The pad owner delivers a jack message although the line is quiet.
    pub fn rearm_jack_for_test(&mut self) {
        self.jack_msgs += 1;
    }

    /// Re-evaluate the codec's interrupt line after a test changed the codec directly.
    pub fn poke_jack_line(&mut self) {
        self.update_jack_line();
    }

    fn update_jack_line(&mut self) {
        let line = self.codec.irq_asserted();
        if line && !self.jack_prev_line && self.jack_armed {
            self.jack_msgs += 1;
            self.jack_armed = false;
        }
        self.jack_prev_line = line;
    }

    pub fn irq_level(&self) -> bool {
        self.force_irq || self.hda.irq() || self.dsp.irq()
    }

    /// Advance the world to `t_us`, running the engines and delivering the events due.
    pub fn advance_to(&mut self, t: u64) {
        if t <= self.t_us {
            return;
        }
        loop {
            // deliver events in time order, running the engines up to each one
            let next = self.evs.iter().enumerate().filter(|(_, e)| e.0 <= t).min_by_key(|(_, e)| (e.0, e.1)).map(|(i, e)| (i, e.0));
            match next {
                Some((i, at)) => {
                    self.run_engines(at);
                    let (_, _, ev) = self.evs.remove(i);
                    self.deliver(ev);
                }
                None => break,
            }
        }
        self.run_engines(t);
    }

    fn run_engines(&mut self, t: u64) {
        let t = t.max(self.t_us);
        self.t_us = t;
        let regions = &self.regions;
        self.hda.advance(t, &|a, n| read_region(regions, a, n));
    }

    fn deliver(&mut self, ev: Ev) {
        match ev {
            Ev::Request(f) => self.inbox.push_back(f),
            Ev::Plug { four_pole } => {
                self.codec.plug(four_pole);
                self.update_jack_line();
            }
            Ev::Unplug => {
                self.codec.unplug();
                self.update_jack_line();
            }
            Ev::Press { button } => {
                self.codec.press(button);
                self.update_jack_line();
            }
            Ev::Release { button } => {
                self.codec.release(button);
                self.update_jack_line();
            }
            Ev::PeerDead => self.client_dead = true,
            Ev::JackOwnerDead => self.jack_owner_dead = true,
            Ev::Custom(f) => f(self),
        }
    }

    fn next_event_us(&self) -> Option<u64> {
        let a = self.evs.iter().map(|e| e.0).min();
        let b = self.hda.next_boundary_us();
        match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn ready(&self, want: Want) -> Option<Wake> {
        if want.avs_irq && self.irq_level() {
            return Some(Wake::AvsIrq);
        }
        if want.accept && self.listen_pending > 0 {
            return Some(Wake::Accept);
        }
        if want.requests && self.client_dead {
            return Some(Wake::PeerDead);
        }
        if want.requests && !self.inbox.is_empty() {
            return Some(Wake::Request);
        }
        if want.jack && self.jack_owner_dead {
            return Some(Wake::JackGone);
        }
        if want.jack && self.jack_msgs > 0 {
            return Some(Wake::Jack);
        }
        None
    }

    // ---- DMA memory ----
    fn region_mut(&mut self, phys: u64, off: usize, len: usize) -> Option<&mut [u8]> {
        for r in self.regions.iter_mut() {
            if phys >= r.phys && phys + (off + len) as u64 <= r.phys + r.data.len() as u64 {
                let o = (phys - r.phys) as usize + off;
                return Some(&mut r.data[o..o + len]);
            }
        }
        None
    }

    /// The samples the hardware read from stream `i`.
    pub fn played(&self, i: usize) -> &[u8] {
        &self.hda.played[i]
    }

    pub fn log_has(&self, needle: &str) -> bool {
        self.logs.iter().any(|l| l.contains(needle))
    }

    pub fn writes_to(&self, space: Space, off: u32) -> Vec<WriteRec> {
        self.writes.iter().filter(|w| w.space == space && w.off == off).copied().collect()
    }

    pub fn amp_touches(&self) -> usize {
        self.i2c.iter().filter(|t| t.addr == 0x31 || t.addr == 0x32).count()
    }

    /// Is the amplifiers' reset line asserted (low) right now? The pad pulls up when its transmitter is off.
    pub fn amp_reset_asserted(&self) -> bool {
        if self.pad_stuck_low {
            return true;
        }
        if self.pad_stuck_high {
            return false;
        }
        let o = 0x400 + 11 * 8;
        let v = u32::from_le_bytes(self.pad2[o..o + 4].try_into().unwrap());
        v & 0x100 == 0 && v & 1 == 0
    }

    fn sync_amp_reset(&mut self) {
        let asserted = self.amp_reset_asserted();
        let seq = self.tx_seq;
        for a in self.amps.iter_mut() {
            a.reset_line(asserted, seq);
        }
    }

    pub fn amp(&self, addr: u16) -> &amp::Amp {
        self.amps.iter().find(|a| a.addr == addr).expect("an amplifier address")
    }

    fn granted(list: &Option<Vec<(u32, u32)>>, off: u32, width: Width) -> bool {
        let w = match width {
            Width::B8 => 1,
            Width::W16 => 2,
            Width::D32 => 4,
        };
        list.as_ref().is_some_and(|l| l.iter().any(|(o, n)| off >= *o && off + w <= o + n))
    }
}

fn w_bytes(width: Width) -> u32 {
    match width {
        Width::B8 => 1,
        Width::W16 => 2,
        Width::D32 => 4,
    }
}

impl Machine for Sim {
    fn reg_read(&mut self, space: Space, off: u32, width: Width) -> Result<u32, RegFault> {
        let n = w_bytes(width);
        let len = match space {
            Space::Hda => hda::LEN,
            Space::Dsp => dsp::LEN,
            Space::PciConfig => 256,
        };
        if off as usize + n as usize > len || !off.is_multiple_of(n) {
            return Err(RegFault::Range { space, off });
        }
        Ok(match space {
            Space::Hda => self.hda.read(off, n),
            Space::Dsp => self.dsp.read(off, n),
            Space::PciConfig => {
                let o = off as usize;
                match n {
                    1 => self.cfg[o] as u32,
                    2 => u16::from_le_bytes([self.cfg[o], self.cfg[o + 1]]) as u32,
                    _ => u32::from_le_bytes(self.cfg[o..o + 4].try_into().unwrap()),
                }
            }
        })
    }

    fn reg_write(&mut self, space: Space, off: u32, width: Width, value: u32) -> Result<(), RegFault> {
        let n = w_bytes(width);
        let len = match space {
            Space::Hda => hda::LEN,
            Space::Dsp => dsp::LEN,
            Space::PciConfig => 256,
        };
        let rec = WriteRec { space, off, width, value };
        if off as usize + n as usize > len || !off.is_multiple_of(n) {
            return Err(RegFault::Range { space, off });
        }
        let grant = match space {
            Space::Hda => &self.grant_hda,
            Space::Dsp => &self.grant_dsp,
            Space::PciConfig => &self.grant_cfg,
        };
        if !Self::granted(grant, off, width) {
            self.refused_writes.push(rec);
            return Err(RegFault::NotAllowed { space, off });
        }
        self.writes.push(rec);
        self.tx_seq += 1;
        let t = self.t_us;
        let regions = &self.regions;
        self.hda.advance(t, &|a, n| read_region(regions, a, n));
        match space {
            Space::Hda => {
                self.hda.write(off, n, value);
                self.hda.after_write(t);
                if self.first_dma_seq.is_none() && self.hda.running(7) {
                    self.first_dma_seq = Some(self.tx_seq);
                }
            }
            Space::Dsp => {
                let regions = &self.regions;
                self.dsp.write(off, n, value, &|a, n| read_region(regions, a, n));
            }
            Space::PciConfig => {
                let o = off as usize;
                self.cfg[o..o + n as usize].copy_from_slice(&value.to_le_bytes()[..n as usize]);
            }
        }
        Ok(())
    }

    fn window_len(&mut self, space: Space) -> u32 {
        match space {
            Space::Hda => hda::LEN as u32,
            Space::Dsp => dsp::LEN as u32,
            Space::PciConfig => 256,
        }
    }

    fn dma_alloc(&mut self, size: usize, align: usize) -> Result<DmaRegion, DmaRefusal> {
        let size = size.div_ceil(4096) * 4096;
        if let Some(b) = self.dma_budget {
            if self.dma_used + size > b {
                return Err(DmaRefusal::OverQuota { len: size });
            }
        }
        self.dma_used += size;
        let phys = self.next_phys.div_ceil(align as u64) * align as u64;
        self.next_phys = phys + size as u64 + 0x1000;
        self.regions.push(Region { phys, data: vec![0; size] });
        Ok(DmaRegion { virt: phys, phys, len: size })
    }

    fn dma_write(&mut self, region: &DmaRegion, off: usize, data: &[u8]) {
        if let Some(d) = self.region_mut(region.phys, off, data.len()) {
            d.copy_from_slice(data);
        }
    }

    fn dma_read(&mut self, region: &DmaRegion, off: usize, out: &mut [u8]) {
        let v = read_region(&self.regions, region.phys + off as u64, out.len());
        out.copy_from_slice(&v);
    }

    fn i2c_write(&mut self, addr: u16, data: &[u8]) -> Result<(), I2cFault> {
        let t = self.t_us + self.i2c_cost_us;
        self.advance_to(t);
        self.tx_seq += 1;
        let seq = self.tx_seq;
        let acked = if addr == codec::ADDR {
            self.codec.write(data)
        } else if let Some(a) = self.amps.iter_mut().find(|a| a.addr == addr) {
            a.write(data, seq)
        } else {
            false
        };
        self.i2c.push(I2cRec { addr, write: data.to_vec(), read_len: 0, acked });
        self.update_jack_line();
        if acked {
            Ok(())
        } else {
            Err(I2cFault::Nak)
        }
    }

    fn i2c_write_read(&mut self, addr: u16, write: &[u8], read: &mut [u8]) -> Result<(), I2cFault> {
        let t = self.t_us + self.i2c_cost_us;
        self.advance_to(t);
        self.tx_seq += 1;
        let seq = self.tx_seq;
        let acked = if addr == codec::ADDR {
            self.codec.write_read(write, read)
        } else if let Some(a) = self.amps.iter_mut().find(|a| a.addr == addr) {
            a.write_read(write, read, seq)
        } else {
            false
        };
        self.i2c.push(I2cRec { addr, write: write.to_vec(), read_len: read.len(), acked });
        if acked {
            Ok(())
        } else {
            Err(I2cFault::Nak)
        }
    }

    fn pad_read(&mut self, reg: PadReg) -> Result<u32, PadFault> {
        if reg.community != 2 {
            return Err(PadFault::NoWindow { community: reg.community });
        }
        let o = reg.off as usize;
        if o + 4 > self.pad2.len() {
            return Err(PadFault::Range { community: 2, off: reg.off });
        }
        let mut v = u32::from_le_bytes(self.pad2[o..o + 4].try_into().unwrap());
        let f10 = 0x400 + 10 * 8;
        let f11 = 0x400 + 11 * 8;
        if o == f10 {
            // the codec's interrupt: RXSTATE is the line's level, LOW when asserted
            v = (v & !2) | if self.codec.irq_asserted() { 0 } else { 2 };
        }
        if o == f11 && v & 0x100 == 0 {
            // transmitter on: the pad follows what is driven
            v = (v & !2) | ((v & 1) << 1);
        }
        if o == f11 && self.pad_stuck_high {
            v |= 2;
        }
        if o == f11 && self.pad_stuck_low {
            v &= !2;
        }
        Ok(v)
    }

    fn pad_write(&mut self, reg: PadReg, value: u32) -> Result<(), PadFault> {
        if reg.community != 2 {
            return Err(PadFault::NoWindow { community: reg.community });
        }
        if !self.pad_granted || reg.off != 0x400 + 11 * 8 {
            return Err(PadFault::NotAllowed { community: 2, off: reg.off });
        }
        self.tx_seq += 1;
        self.pad_writes.push((reg.off, value));
        self.pad_times_us.push(self.t_us);
        let o = reg.off as usize;
        self.pad2[o..o + 4].copy_from_slice(&value.to_le_bytes());
        self.sync_amp_reset();
        Ok(())
    }

    fn now_ms(&mut self) -> u64 {
        self.t_us / 1000
    }

    fn wait(&mut self, deadline_ms: Option<u64>, want: Want) -> Wake {
        let deadline_us = deadline_ms.map(|d| d * 1000);
        let woke = loop {
            if let Some(w) = self.ready(want) {
                break w;
            }
            let next = self.next_event_us();
            let target = match (deadline_us, next) {
                (Some(d), Some(n)) => d.min(n),
                (Some(d), None) => d,
                (None, Some(n)) => n,
                (None, None) => {
                    // nothing could ever wake the task: it is blocked for good (a test's "idle")
                    self.blocked_forever += 1;
                    break Wake::Deadline;
                }
            };
            if target <= self.t_us {
                // a deadline already passed (a non-blocking look) or an event due now
                if deadline_us.is_some_and(|d| d <= self.t_us) && next.is_none_or(|n| n > self.t_us) {
                    break Wake::Deadline;
                }
            }
            self.advance_to(target.max(self.t_us + 1));
            if let Some(w) = self.ready(want) {
                break w;
            }
            if deadline_us.is_some_and(|d| d <= self.t_us) {
                break Wake::Deadline;
            }
        };
        self.waits.push(WaitRec { at_us: self.t_us, deadline_ms, want, woke });
        woke
    }

    fn rearm_avs_irq(&mut self) {
        self.irq_rearms += 1;
    }

    fn take_jack_event(&mut self) -> bool {
        if self.jack_msgs > 0 {
            self.jack_msgs -= 1;
            true
        } else {
            false
        }
    }

    fn rearm_jack(&mut self) {
        self.jack_acks += 1;
        self.jack_armed = true;
        if self.codec.irq_asserted() {
            self.jack_msgs += 1;
            self.jack_armed = false;
        }
    }

    fn accept(&mut self) -> bool {
        if self.listen_pending > 0 && !self.session_open {
            self.listen_pending -= 1;
            self.session_open = true;
            self.client_dead = false;
            true
        } else {
            false
        }
    }

    fn release_session(&mut self) {
        if self.session_open {
            self.session_open = false;
            self.released += 1;
            self.inbox.clear();
        }
    }

    fn recv_request(&mut self, buf: &mut [u8]) -> Recv {
        if self.client_dead {
            return Recv::PeerDead;
        }
        match self.inbox.pop_front() {
            None => Recv::Empty,
            Some(f) if f.len() > buf.len() => Recv::Oversized(f.len() as u64),
            Some(f) => {
                buf[..f.len()].copy_from_slice(&f);
                Recv::Frame(f.len())
            }
        }
    }

    fn send_to_client(&mut self, frame: &[u8]) -> SendOutcome {
        if self.client_dead {
            return SendOutcome::PeerDead;
        }
        if self.ring_full > 0 {
            self.ring_full -= 1;
            return SendOutcome::RingFull;
        }
        self.outbox.push(frame.to_vec());
        SendOutcome::Sent
    }

    fn fw_exchange(&mut self, req: &[u8], reply: &mut [u8], _timeout_ms: u32) -> FwRecv {
        if self.broker_down {
            return FwRecv::Unavailable;
        }
        if self.broker_timeout {
            self.advance_to(self.t_us + 2_000_000);
            return FwRecv::Timeout;
        }
        let Ok((h, payload)) = Header::decode(req) else { return FwRecv::Timeout };
        let n = match h.opcode {
            Opcode::FwStat => {
                let Ok(s) = FwStat::decode(payload) else { return FwRecv::Timeout };
                let name = String::from_utf8_lossy(s.name).into_owned();
                self.fw_requests.push(format!("stat {name}"));
                match self.files.get(&name) {
                    Some(f) => stat_reply(h.request_id, Status::Ok, f.len() as u64, reply),
                    None => stat_reply(h.request_id, Status::FwNotFound, 0, reply),
                }
            }
            Opcode::FwRead => {
                let Ok(r) = FwRead::decode(payload) else { return FwRecv::Timeout };
                let name = String::from_utf8_lossy(r.name).into_owned();
                self.fw_requests.push(format!("read {name} @{}", r.offset));
                match self.files.get(&name) {
                    Some(f) if r.offset + r.len as u64 <= f.len() as u64 => read_reply(h.request_id, Status::Ok, &f[r.offset as usize..(r.offset + r.len as u64) as usize], reply),
                    Some(_) => read_reply(h.request_id, Status::FwRange, &[], reply),
                    None => read_reply(h.request_id, Status::FwNotFound, &[], reply),
                }
            }
            _ => return FwRecv::Timeout,
        };
        match n {
            Ok(n) => FwRecv::Frame(n),
            Err(_) => FwRecv::Timeout,
        }
    }

    fn log(&mut self, line: &str) {
        self.logs.push(line.to_string());
    }

    fn board_loaded(&mut self, _board: &avs_wire::board::Board) {
        self.board_loaded = true;
    }

    fn layout_ready(&mut self, allow: &avs_device::allow::Allow) {
        self.layout_ready_calls += 1;
        self.grant_hda = allow.window_ranges(Space::Hda).ok();
        self.grant_dsp = allow.window_ranges(Space::Dsp).ok();
        self.grant_cfg = allow.window_ranges(Space::PciConfig).ok();
    }
}
