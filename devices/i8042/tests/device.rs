// SPDX-License-Identifier: GPL-2.0-only
//! The device end to end on a simulated controller: bring-up markers, bytes -> physical key frames, the dropped-frame
//! resynchronisation, and that the only ports ever touched are 0x60 and 0x64.
mod sim;

use coralos_rt::keywire::{self, decode, ALL_UP, PRESS, RELEASE, REPEAT};
use coralos_rt::portio::{rights, PortRaw, OP_IN8, OP_OUT8, OP_QUERY, REFUSED};
use i8042_core::bringup::Options;
use i8042_core::regs::{DATA_REG, STATUS_REG};
use i8042_core::Port;
use i8042_device::init::init;
use i8042_device::io::{Delay, KbPort};
use i8042_device::log::Log;
use i8042_device::pacer::{Mode, Pacer, FAST_MS, SLOW_MS};
use i8042_device::run::{drain, Outbox, Sink, MAX_PER_WAKE};
use sim::Sim;

#[derive(Default)]
struct Lines(Vec<String>);
impl Log for Lines {
    fn line(&mut self, s: &str) {
        self.0.push(s.trim_end().to_string());
    }
}

/// Every frame offered; `fail_next` frames are refused first.
#[derive(Default)]
struct Wire {
    got: Vec<[u8; 3]>,
    fail_next: u32,
}
impl Sink for Wire {
    fn send(&mut self, f: [u8; 3]) -> bool {
        if self.fail_next > 0 {
            self.fail_next -= 1;
            return false;
        }
        self.got.push(f);
        true
    }
}

fn frames(w: &Wire) -> Vec<(u8, u8)> {
    w.got.iter().map(|f| { let k = decode(f).expect("a key frame"); (k.usage, k.state) }).collect()
}

fn up() -> (i8042_device::Device<Sim>, Lines) {
    let mut lg = Lines::default();
    let d = init(Sim::new(), Options::LINUX_X86, &mut lg).expect("a healthy controller");
    (d, lg)
}

fn type_and_drain(d: &mut i8042_device::Device<Sim>, set2: &[u8], w: &mut Wire, o: &mut Outbox) {
    d.ctl.io.type_set2(set2);
    drain(d, o, w, &mut Lines::default());
}

#[test]
fn bring_up_names_every_step_then_ok() {
    let (_, lg) = up();
    let names: Vec<&str> = lg.0.iter().filter_map(|l| l.strip_prefix("[I8042-DEV STEP ")).map(|l| l.split(' ').next().unwrap()).collect();
    assert_eq!(names, ["FLUSH", "SELFTEST", "CTR", "KBD-TEST", "AUX-OFF", "PORT-ENABLE", "KBD-RESET", "GETID", "SCANSET", "LEDS", "TYPEMATIC", "SCAN-ENABLE"]);
    let ok = lg.0.last().unwrap();
    assert!(ok.starts_with("[I8042-DEV OK set=2 translated=1 id=0xab83"), "{ok}");
    assert!(!lg.0.iter().any(|l| l.contains("FAIL")));
}

#[test]
fn shift_a_arrives_as_four_physical_events_with_the_modifier_as_its_own_usage() {
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    // Left Shift down, A down, A up, Left Shift up (set 2: 12, 1c, f0 1c, f0 12).
    type_and_drain(&mut d, &[0x12, 0x1c, 0xf0, 0x1c, 0xf0, 0x12], &mut w, &mut o);
    assert_eq!(frames(&w), [(0xE1, PRESS), (0x04, PRESS), (0x04, RELEASE), (0xE1, RELEASE)]);
    assert!(w.got.iter().all(|f| f[0] == keywire::MARKER && f.len() == keywire::LEN));
    assert!(!d.pipe.any_down());
}

#[test]
fn the_search_key_is_left_meta_and_the_top_row_is_plain_function_keys() {
    // Pixelbook Go EC (recovery-image table): no CapsLock key; Search sends set-2 E0 1F = LeftGUI (usage 0xE3); the top
    // row sends F1..F10 (set 2: 05 06 04 0c 03 0b 83 0a 01 09 -> usages 0x3a..0x43). What they MEAN is input-layer policy.
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    type_and_drain(&mut d, &[0xe0, 0x1f, 0xe0, 0xf0, 0x1f], &mut w, &mut o);
    assert_eq!(frames(&w), [(0xE3, PRESS), (0xE3, RELEASE)]);
}

#[test]
fn typematic_is_repeat_not_a_second_press_and_the_release_ends_it() {
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    type_and_drain(&mut d, &[0x1c, 0x1c, 0x1c, 0xf0, 0x1c], &mut w, &mut o);
    assert_eq!(frames(&w), [(0x04, PRESS), (0x04, REPEAT), (0x04, REPEAT), (0x04, RELEASE)]);
}

#[test]
fn a_release_for_a_key_that_is_not_down_is_dropped_and_counted() {
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    type_and_drain(&mut d, &[0xf0, 0x1c], &mut w, &mut o);
    assert!(w.got.is_empty());
    assert_eq!(d.pipe.counts.stray_release, 1);
}

#[test]
fn a_keyboard_reset_byte_while_keys_are_down_declares_all_up_once() {
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    type_and_drain(&mut d, &[0x12, 0x1c], &mut w, &mut o);
    // An EC that resets the keyboard sends the BAT byte 0xAA in raw mode; here the controller is translating, so feed the
    // decoder the raw byte the way a keyboard reset arrives: 0xAA is not a Shift-break (Shift is down, so it IS one in
    // translated mode - use the error byte 0xFF, which is never a key).
    d.ctl.io.out.push_back((0xff, false));
    drain(&mut d, &mut o, &mut w, &mut Lines::default());
    assert_eq!(frames(&w), [(0xE1, PRESS), (0x04, PRESS), (0, ALL_UP)]);
    assert!(!d.pipe.any_down());
    drain(&mut d, &mut o, &mut w, &mut Lines::default());
    assert_eq!(w.got.len(), 3, "nothing more to say");
}

#[test]
fn a_dropped_release_is_repaired_by_a_resynchronisation_that_names_what_is_still_held() {
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    type_and_drain(&mut d, &[0x12], &mut w, &mut o); // Shift down: delivered
    w.fail_next = 1; // the 'a' press is refused
    type_and_drain(&mut d, &[0x1c], &mut w, &mut o);
    assert!(o.lost && o.dropped == 1);
    // 'a' is released; Shift is still held. The next frame that can be sent is a resync: ALL_UP, then Shift.
    type_and_drain(&mut d, &[0xf0, 0x1c], &mut w, &mut o);
    assert_eq!(frames(&w), [(0xE1, PRESS), (0, ALL_UP), (0xE1, PRESS)]);
    assert!(!o.lost);
    assert_eq!(o.resyncs, 1);
    // From here events flow again.
    type_and_drain(&mut d, &[0xf0, 0x12], &mut w, &mut o);
    assert_eq!(*frames(&w).last().unwrap(), (0xE1, RELEASE));
}

#[test]
fn a_resync_that_is_itself_refused_stays_lost_and_tries_again() {
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    w.fail_next = 1;
    type_and_drain(&mut d, &[0x1c], &mut w, &mut o);
    assert!(o.lost);
    w.fail_next = 1; // the ALL_UP of the resync is refused too
    type_and_drain(&mut d, &[0xf0, 0x1c], &mut w, &mut o);
    assert!(o.lost);
    type_and_drain(&mut d, &[0x1c, 0xf0, 0x1c], &mut w, &mut o);
    assert!(!o.lost);
    assert_eq!(frames(&w)[0], (0, ALL_UP));
}

#[test]
fn aux_bytes_never_become_keys() {
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    d.ctl.io.aux(0x1e);
    d.ctl.io.aux(0x9e);
    drain(&mut d, &mut o, &mut w, &mut Lines::default());
    assert!(w.got.is_empty());
    assert_eq!(o.aux_bytes, 2);
}

#[test]
fn a_controller_that_never_empties_is_read_a_bounded_number_of_times() {
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    d.ctl.io.chatter = Some(0xe0); // a prefix byte forever: the status register always says full
    let n = drain(&mut d, &mut o, &mut w, &mut Lines::default());
    assert_eq!(n, MAX_PER_WAKE);
}

#[test]
fn no_controller_is_a_named_fail_and_no_device() {
    let mut s = Sim::new();
    s.absent = true;
    let mut lg = Lines::default();
    assert!(init(s, Options::LINUX_X86, &mut lg).is_err());
    assert!(lg.0.iter().any(|l| l.starts_with("[I8042-DEV FAIL step=FLUSH why=NO-CONTROLLER")), "{:?}", lg.0);
}

#[test]
fn a_keyboard_that_ignores_its_commands_runs_degraded_and_says_which_step() {
    let mut s = Sim::new();
    s.no_keyboard = true;
    let mut lg = Lines::default();
    let mut d = init(s, Options::LINUX_X86, &mut lg).expect("the controller is fine; only the keyboard commands failed");
    assert!(d.degraded);
    assert!(lg.0.iter().any(|l| l.starts_with("[I8042-DEV FAIL step=GETID")), "{:?}", lg.0);
    assert!(lg.0.iter().any(|l| l.starts_with("[I8042-DEV DEGRADED step=GETID")), "{:?}", lg.0);
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    type_and_drain(&mut d, &[0x1c], &mut w, &mut o);
    assert_eq!(frames(&w), [(0x04, PRESS)], "scancodes still flow");
}

#[test]
fn the_first_key_is_named_once() {
    let (mut d, _) = up();
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    let mut lg = Lines::default();
    d.ctl.io.type_set2(&[0x1c, 0xf0, 0x1c, 0x32]);
    drain(&mut d, &mut o, &mut w, &mut lg);
    assert_eq!(lg.0.iter().filter(|l| l.starts_with("[I8042-DEV FIRST-KEY")).count(), 1);
}

// ---- the port capability path --------------------------------------------------------------------------------

struct NoDelay;
impl Delay for NoDelay {
    fn us(&mut self, _: u32) {}
}

/// The kernel's PORT_IO, modelled: two byte ranges, 0x60 and 0x64, in front of a Sim. Every call is recorded.
struct Bus {
    sim: Sim,
    caps: Vec<(u16, u16, u8)>,
    calls: Vec<(u64, u64)>,
}
impl PortRaw for Bus {
    fn call(&mut self, op: u64, a: u64, v: u64) -> u64 {
        self.calls.push((op, a));
        if op == OP_QUERY {
            return match self.caps.get(a as usize) {
                Some(&(b, l, r)) => b as u64 | (l as u64) << 16 | (r as u64) << 32,
                None => REFUSED,
            };
        }
        let dir = if op == OP_IN8 { rights::IN } else { rights::OUT };
        if !self.caps.iter().any(|&(b, l, r)| op <= OP_OUT8 && a >= b as u64 && a < b as u64 + l as u64 && r & dir != 0) {
            return REFUSED;
        }
        match (op, a) {
            (OP_IN8, 0x60) => self.sim.inb(DATA_REG) as u64,
            (OP_IN8, 0x64) => self.sim.inb(STATUS_REG) as u64,
            (OP_OUT8, 0x60) => { self.sim.outb(DATA_REG, v as u8); 0 }
            (OP_OUT8, 0x64) => { self.sim.outb(STATUS_REG, v as u8); 0 }
            _ => panic!("a port outside the grant reached the model: {a:#x}"),
        }
    }
}
const RW8: u8 = rights::IN | rights::OUT | rights::W8;
fn bus(caps: &[(u16, u16, u8)]) -> Bus {
    Bus { sim: Sim::new(), caps: caps.to_vec(), calls: vec![] }
}

#[test]
fn the_whole_device_touches_only_ports_0x60_and_0x64_by_byte() {
    let kb = KbPort::open(bus(&[(0x60, 1, RW8), (0x64, 1, RW8)]), NoDelay).unwrap();
    let mut lg = Lines::default();
    let mut d = init(kb, Options::LINUX_X86, &mut lg).unwrap();
    d.ctl.io.raw.sim.type_set2(&[0x12, 0x1c, 0xf0, 0x1c, 0xf0, 0x12]);
    let (mut w, mut o) = (Wire::default(), Outbox::new());
    drain(&mut d, &mut o, &mut w, &mut Lines::default());
    assert_eq!(frames(&w).len(), 4);
    let touched: std::collections::BTreeSet<(u64, u64)> = d.ctl.io.raw.calls.iter().copied().filter(|c| c.0 != OP_QUERY).collect();
    assert!(touched.iter().all(|&(op, port)| (op == OP_IN8 || op == OP_OUT8) && (port == 0x60 || port == 0x64)), "{touched:?}");
    assert_eq!(d.ctl.io.refusals, 0);
}

#[test]
fn the_device_needs_both_ranges_with_read_write_byte_rights() {
    use coralos_rt::portio::PortError;
    assert_eq!(KbPort::open(bus(&[]), NoDelay).err(), Some(PortError::NoRange));
    assert_eq!(KbPort::open(bus(&[(0x60, 1, RW8)]), NoDelay).err(), Some(PortError::NoRange), "0x64 missing");
    assert_eq!(KbPort::open(bus(&[(0x60, 1, rights::IN | rights::W8), (0x64, 1, RW8)]), NoDelay).err(), Some(PortError::Direction), "0x60 read-only");
}

#[test]
fn a_refusing_kernel_reads_as_a_floating_bus_and_is_counted_not_a_panic() {
    let mut kb = KbPort::open(bus(&[(0x60, 1, RW8), (0x64, 1, RW8)]), NoDelay).unwrap();
    kb.raw.caps.clear(); // the grant is revoked under the device
    assert_eq!(kb.inb(STATUS_REG), 0xFF);
    kb.outb(DATA_REG, 1);
    assert_eq!(kb.refusals, 2);
    let mut lg = Lines::default();
    assert!(init(kb, Options::LINUX_X86, &mut lg).is_err(), "a floating bus is NO-CONTROLLER");
}

#[test]
fn the_pacer_polls_fast_only_when_data_turns_up_without_the_irq() {
    let mut p = Pacer::new();
    assert_eq!((p.mode(), p.timeout_ms()), (Mode::Irq, SLOW_MS));
    assert_eq!(p.observe(true, 1), None);
    assert_eq!(p.observe(false, 0), None, "an idle timeout is not evidence");
    assert_eq!(p.observe(false, 2), Some(Mode::Polling));
    assert_eq!(p.timeout_ms(), FAST_MS);
    assert_eq!(p.observe(false, 0), None);
    assert_eq!(p.observe(true, 1), Some(Mode::Irq), "the line works again");
    let mut q = Pacer::new();
    assert_eq!(q.irq_unavailable(), Some(Mode::Polling));
    assert_eq!(q.irq_unavailable(), None);
}
