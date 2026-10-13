// SPDX-License-Identifier: GPL-2.0-only
//! The test client and the loop that drives the device against the simulated Atlas.
#![allow(dead_code)]
use super::codec::ADDR;
use super::world::*;
use super::{dsp, Ev, Sim};
use avs_device::machine::Wake;
use avs_device::Device;
use avs_topology_core::topology::Topology;
use avs_wire::msg::*;
use avs_wire::*;

pub const BASEFW_BODY: usize = 40_000;

/// A simulated Atlas with every file in place, or None (loudly) when the recovery topology is not on this machine.
pub fn atlas() -> Option<Sim> {
    let tplg = tplg_bytes()?;
    let uuids: Vec<[u8; 16]> = {
        let t = Topology::parse(&tplg, None).unwrap();
        t.modcfgs_ext.iter().map(|e| e.type_uuid).collect()
    };
    let mut sim = Sim::new(dsp::atlas_firmware(&uuids), BASEFW_BODY);
    sim.files.insert("board.bin".into(), board_bytes(&atlas_board()));
    sim.files.insert("skl/dsp_basefw.bin".into(), synth_image(b"BASEFW\0\0", BASEFW_VERSION, BASEFW_BODY, 0x11));
    sim.files.insert("da7219-tplg.bin".into(), tplg);
    sim.files.insert("nhlt.bin".into(), nhlt_bytes());
    Some(sim)
}

pub struct Client {
    pub next_id: u64,
}

impl Client {
    pub fn new() -> Self {
        Self { next_id: 1 }
    }
    pub fn frame(&mut self, op: Opcode, payload: &[u8]) -> (u64, Vec<u8>) {
        let id = self.next_id;
        self.next_id += 1;
        let mut out = vec![0u8; HEADER_BYTES + payload.len()];
        let n = frame(op, 0, id, payload, &mut out).unwrap();
        out.truncate(n);
        (id, out)
    }
}

#[derive(Clone, Debug)]
pub struct Got {
    pub op: Opcode,
    pub id: u64,
    pub reply: bool,
    pub event: bool,
    pub payload: Vec<u8>,
}

pub fn drain(sim: &mut Sim) -> Vec<Got> {
    let frames = std::mem::take(&mut sim.outbox);
    frames
        .iter()
        .map(|f| {
            let (h, p) = Header::decode(f).expect("the device sent a frame the wire cannot decode");
            Got { op: h.opcode, id: h.request_id, reply: h.is_reply(), event: h.is_event(), payload: p.to_vec() }
        })
        .collect()
}

pub fn status_of(g: &Got) -> (Status, Vec<u8>) {
    let (s, b) = split_reply(&g.payload).unwrap();
    (s, b.to_vec())
}

/// Step the device until it has answered request `id`; events seen on the way are returned too.
pub fn call(dev: &mut Device<Sim>, c: &mut Client, op: Opcode, payload: &[u8]) -> (Status, Vec<u8>, Vec<Got>) {
    let (id, f) = c.frame(op, payload);
    dev.rig.m.request(f);
    let mut seen = vec![];
    for _ in 0..20_000 {
        dev.step();
        for g in drain(&mut dev.rig.m) {
            if g.reply && g.id == id {
                let (s, b) = status_of(&g);
                return (s, b, seen);
            }
            seen.push(g);
        }
    }
    panic!("no reply to {op:?}");
}

/// Run the device for `ms` of virtual time (or until it goes idle). Returns the frames the client received.
pub fn run_ms(dev: &mut Device<Sim>, ms: u64) -> Vec<Got> {
    let end = dev.rig.m.t_us + ms * 1000;
    let idle0 = dev.rig.m.blocked_forever;
    let mut out = vec![];
    for _ in 0..200_000 {
        if dev.rig.m.t_us >= end {
            break;
        }
        // a wait with nothing to wake it: advance the clock ourselves so the loop ends
        if dev.rig.m.blocked_forever != idle0 {
            break;
        }
        // the step blocks until something happens; schedule a marker so an otherwise idle device cannot stall the test clock
        dev.step();
        out.extend(drain(&mut dev.rig.m));
    }
    out
}

/// Step until the device would block for good.
pub fn settle(dev: &mut Device<Sim>) -> Vec<Got> {
    let idle0 = dev.rig.m.blocked_forever;
    let mut out = vec![];
    for _ in 0..200_000 {
        dev.step();
        out.extend(drain(&mut dev.rig.m));
        if dev.rig.m.blocked_forever != idle0 {
            return out;
        }
    }
    panic!("the device never went idle");
}

pub fn s16_open() -> Vec<u8> {
    let mut b = [0u8; 32];
    let n = Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 0 }.encode(&mut b).unwrap();
    b[..n].to_vec()
}

pub fn stream_ref(id: u32) -> Vec<u8> {
    let mut b = [0u8; 8];
    let n = StreamRef { stream: id }.encode(&mut b).unwrap();
    b[..n].to_vec()
}

pub fn pcm_payload(stream: u32, data: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 8 + data.len()];
    let n = PcmWrite { stream, data }.encode(&mut b).unwrap();
    b.truncate(n);
    b
}

pub fn subscribe_all(dev: &mut Device<Sim>, c: &mut Client) {
    let mut b = [0u8; 8];
    let n = Subscribe { mask: SUB_ALL }.encode(&mut b).unwrap();
    let (s, _, _) = call(dev, c, Opcode::Subscribe, &b[..n]);
    assert_eq!(s, Status::Ok);
}

pub fn get_status(dev: &mut Device<Sim>, c: &mut Client) -> StatusBody {
    let (s, b, _) = call(dev, c, Opcode::Status, &[]);
    assert_eq!(s, Status::Ok);
    StatusBody::decode(&b).unwrap()
}

pub fn open_stream(dev: &mut Device<Sim>, c: &mut Client) -> u32 {
    let (s, b, _) = call(dev, c, Opcode::StreamOpen, &s16_open());
    assert_eq!(s, Status::Ok, "open");
    OpenReply::decode(&b).unwrap().stream
}

pub fn start_stream(dev: &mut Device<Sim>, c: &mut Client, id: u32) -> (Status, PhaseReply) {
    let (s, b, _) = call(dev, c, Opcode::StreamStart, &stream_ref(id));
    (s, PhaseReply::decode(&b).unwrap())
}

/// A started device with the headphones in.
pub fn started(plugged: bool) -> Option<Device<Sim>> {
    let mut sim = atlas()?;
    if plugged {
        sim.codec.plug(false);
    }
    let mut dev = Device::new(sim);
    dev.start();
    Some(dev)
}

pub fn codec_addr() -> u16 {
    ADDR
}

/// Stereo samples that identify themselves: frame `n` is (n, !n) as little-endian 16-bit.
pub fn ramp(frames: usize, from: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(frames * 4);
    for n in from..from + frames {
        v.extend_from_slice(&(n as u16).to_le_bytes());
        v.extend_from_slice(&(!(n as u16)).to_le_bytes());
    }
    v
}

pub fn schedule_unplug(dev: &mut Device<Sim>, ms: u64) {
    dev.rig.m.at_ms(ms, Ev::Unplug);
}

pub fn wake_name(w: Wake) -> &'static str {
    match w {
        Wake::AvsIrq => "avs-irq",
        Wake::Jack => "jack",
        Wake::Request => "request",
        Wake::PeerDead => "peer-dead",
        Wake::Accept => "accept",
        Wake::JackGone => "jack-gone",
        Wake::Deadline => "deadline",
    }
}
