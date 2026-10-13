// SPDX-License-Identifier: GPL-2.0-only
//! The firmware channel's client against a broker that is correct, slow, wrong, or hostile: nothing it says is believed unchecked.
use avs_device::firmware::*;
use avs_device::machine::*;
use avs_fw_core::effect::{Space, Width};
use avs_wire::fw::*;
use avs_wire::*;
use coralos_hal::{DmaRefusal, DmaRegion};

type Handler = Box<dyn FnMut(&[u8], &mut [u8]) -> FwRecv>;

/// A machine that is nothing but a firmware channel.
struct Chan {
    h: Handler,
    sent: Vec<Vec<u8>>,
}

impl Machine for Chan {
    fn reg_read(&mut self, _: Space, _: u32, _: Width) -> Result<u32, RegFault> {
        unimplemented!()
    }
    fn reg_write(&mut self, _: Space, _: u32, _: Width, _: u32) -> Result<(), RegFault> {
        unimplemented!()
    }
    fn window_len(&mut self, _: Space) -> u32 {
        unimplemented!()
    }
    fn dma_alloc(&mut self, _: usize, _: usize) -> Result<DmaRegion, DmaRefusal> {
        unimplemented!()
    }
    fn dma_write(&mut self, _: &DmaRegion, _: usize, _: &[u8]) {
        unimplemented!()
    }
    fn dma_read(&mut self, _: &DmaRegion, _: usize, _: &mut [u8]) {
        unimplemented!()
    }
    fn i2c_write(&mut self, _: u16, _: &[u8]) -> Result<(), I2cFault> {
        unimplemented!()
    }
    fn i2c_write_read(&mut self, _: u16, _: &[u8], _: &mut [u8]) -> Result<(), I2cFault> {
        unimplemented!()
    }
    fn pad_read(&mut self, _: PadReg) -> Result<u32, PadFault> {
        unimplemented!()
    }
    fn pad_write(&mut self, _: PadReg, _: u32) -> Result<(), PadFault> {
        unimplemented!()
    }
    fn now_ms(&mut self) -> u64 {
        0
    }
    fn wait(&mut self, _: Option<u64>, _: Want) -> Wake {
        unimplemented!()
    }
    fn rearm_avs_irq(&mut self) {}
    fn take_jack_event(&mut self) -> bool {
        false
    }
    fn rearm_jack(&mut self) {}
    fn recv_request(&mut self, _: &mut [u8]) -> Recv {
        unimplemented!()
    }
    fn send_to_client(&mut self, _: &[u8]) -> SendOutcome {
        unimplemented!()
    }
    fn fw_exchange(&mut self, req: &[u8], reply: &mut [u8], _: u32) -> FwRecv {
        self.sent.push(req.to_vec());
        (self.h)(req, reply)
    }
    fn log(&mut self, _: &str) {}
}

fn file(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 + 7) as u8).collect()
}

/// A correct broker serving `data` under any name.
fn honest(data: Vec<u8>) -> Handler {
    Box::new(move |req, out| {
        let (h, p) = Header::decode(req).unwrap();
        let n = match h.opcode {
            Opcode::FwStat => stat_reply(h.request_id, Status::Ok, data.len() as u64, out).unwrap(),
            Opcode::FwRead => {
                let r = FwRead::decode(p).unwrap();
                read_reply(h.request_id, Status::Ok, &data[r.offset as usize..(r.offset + r.len as u64) as usize], out).unwrap()
            }
            _ => panic!(),
        };
        FwRecv::Frame(n)
    })
}

fn chan(h: Handler) -> Chan {
    Chan { h, sent: vec![] }
}

#[test]
fn a_file_larger_than_a_slice_arrives_whole_and_in_order() {
    let data = file(100_000);
    let mut m = chan(honest(data.clone()));
    let got = fetch(&mut m, "skl/dsp_basefw.bin", 1 << 20).unwrap();
    assert_eq!(got, data);
    // one stat then ceil(100000 / 16384) = 7 reads, each for at most a slice, at consecutive offsets
    assert_eq!(m.sent.len(), 1 + 7);
    let mut next = 0u64;
    for f in &m.sent[1..] {
        let (h, p) = Header::decode(f).unwrap();
        assert_eq!(h.opcode, Opcode::FwRead);
        let r = FwRead::decode(p).unwrap();
        assert_eq!(r.offset, next);
        assert!(r.len <= FW_SLICE);
        assert_eq!(r.name, b"skl/dsp_basefw.bin");
        next += r.len as u64;
    }
    assert_eq!(next, 100_000);
    // exactly a multiple of a slice, one byte, and empty
    for n in [FW_SLICE as usize, FW_SLICE as usize * 2, 1, 0] {
        let mut m = chan(honest(file(n)));
        assert_eq!(fetch(&mut m, "x", 1 << 20).unwrap().len(), n);
    }
}

#[test]
fn a_name_that_is_not_safe_is_refused_before_anything_is_sent() {
    for bad in ["", "/etc/passwd", "../x", "a//b", "a b", &"x".repeat(MAX_FW_NAME + 1)] {
        let mut m = chan(honest(file(10)));
        assert_eq!(fetch(&mut m, bad, 100), Err(FwError::BadName), "{bad:?}");
        assert!(m.sent.is_empty());
    }
}

#[test]
fn a_file_over_the_validation_limit_is_refused_after_the_stat_and_before_any_read() {
    let mut m = chan(honest(file(5000)));
    assert_eq!(fetch(&mut m, "x", 4999), Err(FwError::TooLarge { size: 5000, max: 4999 }));
    assert_eq!(m.sent.len(), 1, "only the stat went out");
    // a size so large that reserving it would abort is a named refusal, not a crash
    let mut m = chan(Box::new(|req, out| {
        let (h, _) = Header::decode(req).unwrap();
        FwRecv::Frame(stat_reply(h.request_id, Status::Ok, u64::MAX, out).unwrap())
    }));
    assert_eq!(fetch(&mut m, "x", u64::MAX), Err(FwError::OutOfMemory));
}

#[test]
fn a_broker_that_says_not_found_or_refuses_is_named() {
    let mut m = chan(Box::new(|req, out| {
        let (h, _) = Header::decode(req).unwrap();
        FwRecv::Frame(stat_reply(h.request_id, Status::FwNotFound, 0, out).unwrap())
    }));
    assert_eq!(fetch(&mut m, "x", 100), Err(FwError::NotFound));
    let mut m = chan(Box::new(|req, out| {
        let (h, _) = Header::decode(req).unwrap();
        FwRecv::Frame(stat_reply(h.request_id, Status::Unsupported, 0, out).unwrap())
    }));
    assert_eq!(fetch(&mut m, "x", 100), Err(FwError::Refused(Status::Unsupported)));
    // not found in the middle of a read
    let mut calls = 0;
    let mut m = chan(Box::new(move |req, out| {
        let (h, _) = Header::decode(req).unwrap();
        calls += 1;
        FwRecv::Frame(if calls == 1 { stat_reply(h.request_id, Status::Ok, 10, out).unwrap() } else { read_reply(h.request_id, Status::FwRange, &[], out).unwrap() })
    }));
    assert_eq!(fetch(&mut m, "x", 100), Err(FwError::Refused(Status::FwRange)));
}

#[test]
fn a_broker_that_is_down_or_silent_is_unavailable_or_a_timeout() {
    let mut m = chan(Box::new(|_, _| FwRecv::Unavailable));
    assert_eq!(fetch(&mut m, "x", 100), Err(FwError::Unavailable));
    let mut m = chan(Box::new(|_, _| FwRecv::Timeout));
    assert_eq!(fetch(&mut m, "x", 100), Err(FwError::Timeout));
}

#[test]
fn replies_that_do_not_match_the_request_are_not_believed() {
    let honest_data = file(40_000);
    type Tamper = fn(&mut Vec<u8>);
    let cases: [(&str, Tamper); 6] = [
        ("wrong request id", |f| f[16] ^= 1),
        ("a request flag instead of a reply flag", |f| f[12] = 0),
        ("an event flag too", |f| f[12] |= 2),
        ("the wrong opcode", |f| f[8] = Opcode::Hello as u8),
        ("a bad magic", |f| f[0] = b'X'),
        ("a reserved word", |f| f[28] = 1),
    ];
    for (what, tamper) in cases {
        let data = honest_data.clone();
        let mut inner = honest(data);
        let mut m = chan(Box::new(move |req, out| {
            let r = inner(req, out);
            if let FwRecv::Frame(n) = r {
                let mut v = out[..n].to_vec();
                tamper(&mut v);
                out[..n].copy_from_slice(&v);
            }
            r
        }));
        let e = fetch(&mut m, "x", 1 << 20).unwrap_err();
        assert!(matches!(e, FwError::BadReply(_)), "{what}: {e:?}");
    }
}

#[test]
fn a_short_or_long_slice_is_refused_as_a_bad_reply() {
    for delta in [-1i64, 1] {
        let data = file(40_000);
        let mut calls = 0;
        let mut m = chan(Box::new(move |req, out| {
            let (h, p) = Header::decode(req).unwrap();
            calls += 1;
            FwRecv::Frame(if h.opcode == Opcode::FwStat {
                stat_reply(h.request_id, Status::Ok, data.len() as u64, out).unwrap()
            } else {
                let r = FwRead::decode(p).unwrap();
                let want = (r.len as i64 + if calls == 3 { delta } else { 0 }) as usize;
                let off = r.offset as usize;
                let slice: Vec<u8> = (0..want).map(|i| data.get(off + i).copied().unwrap_or(0)).collect();
                read_reply(h.request_id, Status::Ok, &slice, out).unwrap()
            })
        }));
        assert_eq!(fetch(&mut m, "x", 1 << 20), Err(FwError::BadReply("slice length")), "delta {delta}");
    }
}

#[test]
fn the_whole_set_is_loaded_and_checked_and_the_first_bad_file_is_named() {
    // the set against a simulated broker is covered by protocol.rs; here: an empty broker names the board record first
    let mut m = chan(Box::new(|req, out| {
        let (h, _) = Header::decode(req).unwrap();
        FwRecv::Frame(stat_reply(h.request_id, Status::FwNotFound, 0, out).unwrap())
    }));
    let e = load_all(&mut m).err().unwrap();
    assert_eq!((e.file, e.why), ("basefw", "fw-not-found"));
}
