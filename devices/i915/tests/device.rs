// SPDX-License-Identifier: GPL-2.0-only
//! The device end to end on simulated windows: identity, refusals, the readout lines, and that nothing is written or even
//! read when the identity is wrong.
use coralos_rt::mmio::MmioError;
use i915_device::identity::{check, read_identity, PciIdentity, Refusal, PCI_BASE_CLASS_DISPLAY};
use i915_device::log::Log;
use i915_device::regio::{RegRead, READOUT_WINDOW_BYTES};
use i915_device::startup::{startup, StartError};
use i915_device::wire::answer;
use i915_kbl_display_core::platform::PlatformError;
use std::collections::BTreeMap;

#[derive(Default)]
struct Lines(Vec<String>);
impl Log for Lines {
    fn line(&mut self, s: &str) {
        self.0.push(s.trim_end().to_string());
    }
}
impl Lines {
    fn has(&self, needle: &str) -> bool {
        self.0.iter().any(|l| l.contains(needle))
    }
    fn find(&self, prefix: &str) -> Vec<&String> {
        self.0.iter().filter(|l| l.starts_with(prefix)).collect()
    }
}

/// A strict, read-only register window: reads of unmapped addresses PANIC, `fail_at` makes one address refuse.
struct Window {
    len: u64,
    vals: BTreeMap<u32, u32>,
    reads: Vec<u32>,
    fail_at: Option<u32>,
}
impl Window {
    fn new(len: u64) -> Self {
        Window { len, vals: BTreeMap::new(), reads: vec![], fail_at: None }
    }
    fn set(&mut self, a: u32, v: u32) -> &mut Self {
        self.vals.insert(a, v);
        self
    }
}
impl RegRead for Window {
    fn len(&self) -> u64 {
        self.len
    }
    fn read32(&mut self, off: u32) -> Result<u32, MmioError> {
        self.reads.push(off);
        if self.fail_at == Some(off) {
            return Err(MmioError::OutOfRange { off: off as u64, width: 4, len: self.len });
        }
        match self.vals.get(&off) {
            Some(v) => Ok(*v),
            None => panic!("read of unmapped register {off:#x}"),
        }
    }
}

/// The PCI configuration header of device `id`: vendor 8086, revision `rev`, class 03:00:00.
fn config_page(device: u16, rev: u8) -> Window {
    config_full(0x8086, device, rev, 0x03, 0x00)
}
fn config_full(vendor: u16, device: u16, rev: u8, base: u8, sub: u8) -> Window {
    let mut w = Window::new(256);
    w.set(0x00, (device as u32) << 16 | vendor as u32);
    w.set(0x08, (base as u32) << 24 | (sub as u32) << 16 | rev as u32);
    w.set(0x2c, 0x1234_5678);
    w
}

/// BAR0 as the firmware leaves a Pixelbook Go-like panel: vector A of the core's tests (1080p60 eDP on DPLL0).
fn bar_vector_a() -> Window {
    let mut r = Window::new(0x40_0000);
    let mut s = |a: u32, v: u32| {
        r.set(a, v);
    };
    s(0x45400, 0x3000_0003);
    s(0x45404, 0xc000_000c);
    s(0x42000, 0x8e00_0000);
    s(0x45504, 0);
    for a in [0x46014, 0x46040, 0x46060, 0x6c040, 0x6c044, 0x6c048, 0x6c04c, 0x6c050, 0x6c054] {
        s(a, 0);
    }
    for a in [0x64100, 0x64200, 0x64300, 0x64400] {
        s(a, 0);
    }
    s(0x46010, 0xc000_0000);
    s(0x6c058, 0x3);
    s(0x6c05c, 0x1);
    s(0x6c060, 0x1);
    s(0x46000, 0x382);
    s(0x64000, 0x8000_0002);
    for t in [0x60400u32, 0x61400, 0x62400] {
        s(t, 0);
    }
    s(0x6f400, 0x8203_0002);
    s(0x7f008, 0x8000_0000);
    for (i, v) in [0x0897_077f, 0x0897_077f, 0x0803_07d7, 0x0464_0437, 0x0464_0437, 0x0440_043b].iter().enumerate() {
        s(0x6f000 + 4 * i as u32, *v);
    }
    s(0x6001c, 0x077f_0437);
    s(0x6f030, 0x7e69_9999);
    s(0x6f034, 0x0080_0000);
    s(0x6f040, 288_358);
    s(0x6f044, 0x0008_0000);
    for pipe_base in [0x70000u32, 0x71000, 0x72000] {
        for plane in 0..3u32 {
            for off in [0x180u32, 0x188, 0x18c, 0x190, 0x19c, 0x1a4] {
                s(pipe_base + plane * 0x100 + off, 0);
            }
        }
    }
    s(0x70180, 0x8400_0400);
    s(0x70188, 15);
    s(0x70190, 0x0437_077f);
    s(0x7019c, 0x0010_0005);
    for (base, n) in [(0x68000u32, 2u32), (0x68800, 2), (0x69000, 1)] {
        for id in 0..n {
            for off in [0x170u32, 0x174, 0x180] {
                s(base + id * 0x100 + off, 0);
            }
        }
    }
    r
}

#[test]
fn the_pixelbook_go_reads_out_and_logs_one_line_per_pipe() {
    let (mut bar, mut cfg, mut lg) = (bar_vector_a(), config_page(0x591c, 0x02), Lines::default());
    let ready = startup(&mut bar, &mut cfg, &mut lg).expect("a Kaby Lake with a firmware-lit panel");
    assert_eq!(ready.active_pipes, 1);
    assert_eq!(ready.identity.device, 0x591c);
    assert_eq!(ready.device.id.gt, 2);

    assert_eq!(lg.0[0], "[I915-KBL-DEV START vendor=8086 device=591c rev=02 class=03:00 subsys=5678:1234]");
    assert_eq!(
        lg.0[1],
        "[I915-KBL-DEV IDENTIFIED platform=kabylake gt=2 sub=ulx step=B0 display_ver=9 pipes=3 ports=5 scalers=2,2,1]"
    );
    assert_eq!(lg.0[2], "[I915-KBL-DEV BAR0 len=0x400000 access=read-only]");

    let pipes = lg.find("[I915-KBL-READOUT pipe=");
    assert_eq!(pipes.len(), 3, "exactly one line per pipe");
    assert_eq!(
        pipes[0].as_str(),
        "[I915-KBL-READOUT pipe=A active=1 1920x1080@60 trans=EDP ddi=A lanes=2 rate=2700 pll=0 plane=XR24 tiling=X stride=7680 size=1920x1080 surf=0x100000]"
    );
    assert_eq!(pipes[1].as_str(), "[I915-KBL-READOUT pipe=B active=0 state=disabled plane=off]");
    assert_eq!(pipes[2].as_str(), "[I915-KBL-READOUT pipe=C active=0 state=disabled plane=off]");

    assert!(lg.has(
        "[I915-KBL-READOUT-TIMING pipe=A trans=EDP h=1920/2200 hs=2008-2052 hb=1920-2200 v=1080/1125 vs=1084-1089 vb=1080-1125 dot_khz=148500 mhz=60000 il=0 mult=1 bpp=24 hsync=+ vsync=+ src=1920x1080 dm=6920601/8388608 lm=288358/524288 tu=64]"
    ));
    assert!(lg.has("[I915-KBL-READOUT-CDCLK cdclk_khz=450000 vco_khz=8100000 ref_khz=24000 voltage=1 decimal=0x382 anomaly=none]"));
    // wells in the order PW1, PW2, MISC_IO, DDI_A_E, DDI_B, DDI_C, DDI_D; r = request bit, s = state bit. PW1 and MISC_IO
    // are requested through the BIOS register (CTL1), PW2 and DDI_A_E through the driver register (CTL2).
    assert!(lg.has(
        "[I915-KBL-READOUT-POWER pw1=1 pw2=1 misc_io=1 ddi_ae=1 ddi_b=0 ddi_c=0 ddi_d=0 fuse=0x8e000000 dc_state=off dc9=0 ctl_bios=rs,--,rs,--,--,--,-- ctl_drv=--,rs,--,rs,--,--,--]"
    ));
    assert!(lg.has("[I915-KBL-READOUT-DDI ddi=A buf=on idle=0 lanes=2 reversed=0 a4=0 emp=0 clk_override=1 clk_sel=dpll0 clk_off=0]"));
    assert!(lg.has("[I915-KBL-READOUT-DPLL id=0 enabled=1 locked=1 mode=dp ssc=0 rate_field=1 port_khz=270000 ctrl1=0x03]"));
    // idle DDIs (B..E) and idle PLLs print nothing
    assert!(!lg.has("ddi=B"));
    assert!(!lg.has("DPLL id=1"));
    let last = lg.0.last().unwrap();
    assert!(last.starts_with("[I915-KBL-DEV READY pipes_active=1 reads="));
    assert!(last.ends_with("writes=0 refusals=0]"), "{last}");
    // every register read was inside the display block and dword aligned; the same register is never needed twice
    assert!(bar.reads.iter().all(|a| a % 4 == 0 && (*a as u64) < READOUT_WINDOW_BYTES));
    assert_eq!(ready.reads as usize, bar.reads.len());
    // the config page was read at exactly the three header dwords
    assert_eq!(cfg.reads, vec![0x00, 0x08, 0x2c]);
}

#[test]
fn a_non_kaby_lake_id_is_refused_by_name_and_bar0_is_never_touched() {
    for (vendor, device, why, want) in [
        (0x8086u16, 0x22b0u16, "not-kaby-lake", Refusal::Platform(PlatformError::NotKabyLake { device: 0x22b0 })), // Cherry Trail
        (0x8086, 0x1912, "not-kaby-lake", Refusal::Platform(PlatformError::NotKabyLake { device: 0x1912 })),
        (0x10de, 0x591c, "not-intel", Refusal::Platform(PlatformError::NotIntel { vendor: 0x10de })),
    ] {
        let (mut bar, mut cfg, mut lg) = (Window::new(0x40_0000), config_full(vendor, device, 2, 3, 0), Lines::default());
        let e = startup(&mut bar, &mut cfg, &mut lg).err().expect("must refuse");
        assert_eq!(e, StartError::Refused(want));
        assert_eq!(e.name(), why);
        assert!(lg.has(&format!("[I915-KBL-DEV REFUSED step=IDENTITY why={} vendor={:04x} device={:04x}]", why, vendor, device)), "{:?}", lg.0);
        assert!(bar.reads.is_empty(), "BAR0 must not be read for a refused id");
        assert!(!lg.has("READOUT"));
    }
}

#[test]
fn class_and_config_refusals() {
    // Kaby Lake id but not a display controller class
    let (mut bar, mut cfg, mut lg) = (Window::new(0x40_0000), config_full(0x8086, 0x591c, 2, 0x06, 0x00), Lines::default());
    let e = startup(&mut bar, &mut cfg, &mut lg).err().unwrap();
    assert_eq!(e, StartError::Refused(Refusal::NotDisplayClass { base_class: 6 }));
    assert!(lg.has("why=not-display-class"));
    assert!(bar.reads.is_empty());
    // a dead config page reads all-ones
    let (mut bar, mut cfg, mut lg) = (Window::new(0x40_0000), config_full(0xffff, 0xffff, 0xff, 0xff, 0xff), Lines::default());
    let e = startup(&mut bar, &mut cfg, &mut lg).err().unwrap();
    assert_eq!(e, StartError::Refused(Refusal::ConfigAllOnes));
    assert!(lg.has("[I915-KBL-DEV REFUSED step=CONFIG why=config-all-ones]"));
    // a config window shorter than the 64-byte header
    let mut short = Window::new(32);
    let (mut bar, mut lg) = (Window::new(0x40_0000), Lines::default());
    assert!(matches!(startup(&mut bar, &mut short, &mut lg).err().unwrap(), StartError::Refused(Refusal::ConfigUnreadable(MmioError::TooShort { .. }))));
    assert!(short.reads.is_empty());
    // a config read the window refuses
    let mut c = config_full(0x8086, 0x591c, 2, 3, 0);
    c.fail_at = Some(0x08);
    let (mut bar, mut lg) = (Window::new(0x40_0000), Lines::default());
    assert!(matches!(startup(&mut bar, &mut c, &mut lg).err().unwrap(), StartError::Refused(Refusal::ConfigUnreadable(_))));
}

#[test]
fn a_bar0_shorter_than_the_display_block_is_refused_before_any_read() {
    let (mut bar, mut cfg, mut lg) = (Window::new(READOUT_WINDOW_BYTES - 4), config_page(0x591c, 2), Lines::default());
    let e = startup(&mut bar, &mut cfg, &mut lg).err().unwrap();
    assert_eq!(e, StartError::Bar0(MmioError::TooShort { slot: 0, len: READOUT_WINDOW_BYTES - 4, need: READOUT_WINDOW_BYTES }));
    assert_eq!(e.name(), "MMIO-TOO-SHORT");
    assert!(bar.reads.is_empty());
    assert!(lg.has("[I915-KBL-DEV FAIL step=BAR0 why=MMIO-TOO-SHORT len=0x7fffc need=0x80000]"));
    // exactly the display block long is enough
    let (mut bar, mut lg) = (Window::new(READOUT_WINDOW_BYTES), Lines::default());
    // (an all-idle snapshot would need every register; only the length gate is under test here, so stop at the first read)
    bar.fail_at = Some(0x45400);
    let mut c = config_page(0x591c, 2);
    let e = startup(&mut bar, &mut c, &mut lg).err().unwrap();
    assert!(matches!(e, StartError::ReadRefused { off: 0x45400, .. }));
    assert_eq!(bar.reads, vec![0x45400], "after the first refusal the window is not touched again");
}

#[test]
fn a_refused_bar0_read_discards_the_readout() {
    let mut bar = bar_vector_a();
    bar.fail_at = Some(0x6f400); // the eDP transcoder's TRANS_DDI_FUNC_CTL
    let (mut cfg, mut lg) = (config_page(0x591c, 2), Lines::default());
    let e = startup(&mut bar, &mut cfg, &mut lg).err().unwrap();
    assert!(matches!(e, StartError::ReadRefused { off: 0x6f400, err: MmioError::OutOfRange { .. } }));
    assert!(lg.has("[I915-KBL-DEV FAIL step=READOUT why=MMIO-OUT-OF-RANGE off=0x6f400]"));
    let at = bar.reads.iter().position(|a| *a == 0x6f400).unwrap();
    assert_eq!(at + 1, bar.reads.len(), "the refused read is the last read made");
    assert!(!lg.has("[I915-KBL-READOUT"), "a partial readout must not be presented as a result");
    assert!(!lg.has("READY"));
}

#[test]
fn an_unpowered_pipe_is_named_not_read() {
    let mut bar = bar_vector_a();
    bar.set(0x45404, 0x0000_000c); // PW2 off in the driver request register
    // everything in PW2's domain is now off-limits; remove the values so a stray read panics
    for a in [0x60400u32, 0x61400, 0x62400, 0x71008, 0x72008] {
        bar.vals.remove(&a);
    }
    for pipe_base in [0x71000u32, 0x72000] {
        for plane in 0..3u32 {
            for off in [0x180u32, 0x188, 0x18c, 0x190, 0x19c, 0x1a4] {
                bar.vals.remove(&(pipe_base + plane * 0x100 + off));
            }
        }
    }
    for a in [0x68980u32, 0x68a80, 0x69180, 0x68970, 0x68a70, 0x69170, 0x68974, 0x68a74, 0x69174, 0x68984, 0x68a84, 0x69184, 0x64100, 0x64200, 0x64300, 0x64400] {
        bar.vals.remove(&a);
    }
    let (mut cfg, mut lg) = (config_page(0x591c, 2), Lines::default());
    startup(&mut bar, &mut cfg, &mut lg).expect("pipe A still reads through the always-on eDP transcoder");
    assert!(lg.has("[I915-KBL-READOUT pipe=B active=0 state=not-powered]"));
    assert!(lg.has("[I915-KBL-READOUT pipe=C active=0 state=not-powered]"));
    assert!(lg.has("[I915-KBL-READOUT pipe=A active=1 1920x1080@60"));
    assert!(lg.has("pw2=0"));
}

#[test]
fn identity_fields_come_from_the_standard_pci_header() {
    // distinct values in every field: vendor/device 16-bit halves, revision, class bytes, subsystem halves
    let mut w = Window::new(64);
    w.set(0x00, 0xbeef_8086).set(0x08, 0x0302_01a5).set(0x2c, 0xcafe_f00d);
    let id = read_identity(&mut w).unwrap();
    assert_eq!(
        id,
        PciIdentity { vendor: 0x8086, device: 0xbeef, revision: 0xa5, base_class: 0x03, sub_class: 0x02, subsystem_vendor: 0xf00d, subsystem_device: 0xcafe }
    );
    assert_eq!(PCI_BASE_CLASS_DISPLAY, 3);
    // check(): a valid Kaby Lake id with the right class passes, every family member of the table
    let ok = PciIdentity { vendor: 0x8086, device: 0x591c, revision: 1, base_class: 3, sub_class: 0, subsystem_vendor: 0, subsystem_device: 0 };
    assert!(check(&ok).is_ok());
    assert_eq!(check(&PciIdentity { base_class: 4, ..ok }), Err(Refusal::NotDisplayClass { base_class: 4 }));
    assert_eq!(check(&PciIdentity { base_class: 0, ..ok }), Err(Refusal::NotDisplayClass { base_class: 0 }));
    // the platform refusal is checked before the class (a foreign id is named as foreign whatever its class)
    assert_eq!(check(&PciIdentity { device: 0x22b0, base_class: 4, ..ok }), Err(Refusal::Platform(PlatformError::NotKabyLake { device: 0x22b0 })));
}

#[test]
fn refusal_names_are_distinct_and_stable() {
    let names = [
        Refusal::ConfigUnreadable(MmioError::NoWindow { slot: 1 }).name(),
        Refusal::ConfigAllOnes.name(),
        Refusal::NotDisplayClass { base_class: 0 }.name(),
        Refusal::Platform(PlatformError::NotKabyLake { device: 0 }).name(),
        Refusal::Platform(PlatformError::NotIntel { vendor: 0 }).name(),
    ];
    assert_eq!(names, ["config-unreadable", "config-all-ones", "not-display-class", "not-kaby-lake", "not-intel"]);
}

#[test]
fn wire_answers_everything_and_supports_nothing_but_hello() {
    use i915_device::wire::*;
    assert_eq!((SERVICE_EP, REPLY_EP, REQ_MARKER, REP_MARKER, VERSION, REPLY_LEN), (13, 14, 0x49, 0x69, 1, 4));
    // HELLO is OK; every other defined op is explicitly UNSUPPORTED, with its op echoed
    assert_eq!(answer(&[b'I', 1, op::HELLO]), [b'i', 1, 0, status::OK]);
    for (o, name) in [(op::GET_READOUT, 1u8), (op::SET_MODE, 2), (op::PAGE_FLIP, 3), (op::ALLOC_SCANOUT, 4), (op::SUBMIT_BATCH, 5)] {
        assert_eq!(o, name);
        assert_eq!(answer(&[b'I', 1, o, 9, 9]), [b'i', 1, o, status::UNSUPPORTED], "op {o}");
    }
    // an undefined op is also UNSUPPORTED, not silence
    assert_eq!(answer(&[b'I', 1, 0x77]), [b'i', 1, 0x77, status::UNSUPPORTED]);
    // malformed: too short, wrong lead byte, empty
    for bad in [&[][..], &[b'I'][..], &[b'I', 1][..], &[b'X', 1, 0][..], &[0u8, 1, 0][..]] {
        assert_eq!(answer(bad), [b'i', 1, op::NONE, status::MALFORMED], "{bad:?}");
    }
    // a version we do not speak
    assert_eq!(answer(&[b'I', 2, op::SET_MODE]), [b'i', 1, op::SET_MODE, status::BAD_VERSION]);
    assert_eq!(answer(&[b'I', 0, op::HELLO]), [b'i', 1, op::HELLO, status::BAD_VERSION]);
    // the status values
    assert_eq!((status::OK, status::UNSUPPORTED, status::MALFORMED, status::BAD_VERSION), (0, 1, 2, 3));
    assert_eq!(op::NONE, 0xff);
}

// ---- report lines for the states vector A does not reach ----

fn run_lines(bar: &mut Window) -> Lines {
    let (mut c, mut lg) = (config_page(0x591c, 2), Lines::default());
    startup(bar, &mut c, &mut lg).expect("startup");
    lg
}

#[test]
fn func_on_pipe_off_and_error_states_are_inactive_lines() {
    let mut bar = bar_vector_a();
    bar.set(0x7f008, 0); // TRANSCONF(EDP) off while the eDP function is still enabled
    let lg = run_lines(&mut bar);
    assert!(lg.has("[I915-KBL-READOUT pipe=A active=0 state=func-on-pipe-off trans=EDP]"), "{:?}", lg.0);
    let mut bar = bar_vector_a();
    bar.set(0x6f400, 0x8203_3002); // eDP input field 3 (reserved)
    let lg = run_lines(&mut bar);
    assert!(lg.has("[I915-KBL-READOUT pipe=A active=0 state=error why=edp-input-reserved field=0x3]"), "{:?}", lg.0);
}

#[test]
fn an_unknown_dotclock_prints_a_question_mark_not_a_zero() {
    let mut bar = bar_vector_a();
    bar.set(0x6c05c, 0); // DDI A's clock-select override cleared: no PLL, so no port clock
    let lg = run_lines(&mut bar);
    assert!(lg.has("[I915-KBL-READOUT pipe=A active=1 1920x1080@? trans=EDP ddi=A lanes=2 rate=?(no-pll) pll=none"), "{:?}", lg.0);
    assert!(lg.has("dot_khz=? mhz=?"));
    // a PLL that is selected but off is named on the rate field and warned about
    let mut bar = bar_vector_a();
    bar.set(0x46010, 0x4000_0000);
    let lg = run_lines(&mut bar);
    assert!(lg.has("rate=?(pll-not-enabled) pll=0"));
    assert!(lg.has("[I915-KBL-READOUT-WARN pipe=A pll-error pll-not-enabled ddi=A dpll=0]"));
}

#[test]
fn sprite_planes_scalers_and_errors_get_their_own_lines() {
    let mut bar = bar_vector_a();
    // plane 2 of pipe A enabled: linear XRGB8888 with alpha, 64x64 at surface 0x300000, stride 4 * 64 = 256 = 4 units
    bar.set(0x70280, 0x8400_0020).set(0x70288, 4).set(0x70290, 0x003f_003f).set(0x7029c, 0x0030_0000);
    // plane 3 of pipe A: reserved tiling (field 2)
    bar.set(0x70380, 0x8400_0800);
    // scaler 0 of pipe A bound to plane 2 (field 2: PS_BINDING_PLANE(1)), scaler 1 bound to the pipe (field 0); 1920x1080 windows
    bar.set(0x68180, 0x8000_0000 | (2 << 25)).set(0x68170, 0x0010_0020).set(0x68174, 0x0780_0438);
    bar.set(0x68280, 0x8000_0000).set(0x68270, 0).set(0x68274, 0x0780_0438);
    let lg = run_lines(&mut bar);
    assert!(lg.has("[I915-KBL-READOUT-PLANE pipe=A id=2 plane=AR24 tiling=linear stride=256 size=64x64 surf=0x300000]"), "{:?}", lg.0);
    assert!(lg.has("[I915-KBL-READOUT-PLANE pipe=A id=3 error tiling-reserved plane=3 field=0x2]"), "{:?}", lg.0);
    assert!(lg.has("[I915-KBL-READOUT-SCALER pipe=A id=0 binding=plane2 win=1920x1080+16+32]"), "{:?}", lg.0);
    assert!(lg.has("[I915-KBL-READOUT-SCALER pipe=A id=1 binding=pipe win=1920x1080+0+0]"), "{:?}", lg.0);
    // a disabled scaler or plane prints nothing
    assert!(!lg.has("pipe=B id="));
    assert!(!lg.has("READOUT-PLANE pipe=A id=1"), "the primary plane is on the pipe line, not repeated");
}

#[test]
fn an_unreadable_ddi_with_a_clock_route_is_reported_not_dropped() {
    let mut bar = bar_vector_a();
    bar.set(0x45404, 0x0000_000c); // PW2 off: DDI B..E buffers cannot be read
    for a in [0x60400u32, 0x61400, 0x62400, 0x71008, 0x72008, 0x64100, 0x64200, 0x64300, 0x64400] {
        bar.vals.remove(&a);
    }
    for pipe_base in [0x71000u32, 0x72000] {
        for plane in 0..3u32 {
            for off in [0x180u32, 0x188, 0x18c, 0x190, 0x19c, 0x1a4] {
                bar.vals.remove(&(pipe_base + plane * 0x100 + off));
            }
        }
    }
    for a in [0x68980u32, 0x68a80, 0x69180, 0x68970, 0x68a70, 0x69170, 0x68974, 0x68a74, 0x69174, 0x68984, 0x68a84, 0x69184] {
        bar.vals.remove(&a);
    }
    bar.set(0x6c05c, 0x0000_0001 | 0x0000_0040 | (2 << 7)); // DDI A -> DPLL0; DDI C override, select DPLL2
    let lg = run_lines(&mut bar);
    assert!(lg.has("[I915-KBL-READOUT-DDI ddi=C buf=unreadable-not-powered clk_override=1 clk_sel=dpll2]"), "{:?}", lg.0);
    // DDIs with neither a buffer nor a route print nothing
    assert!(!lg.has("ddi=B") && !lg.has("ddi=D") && !lg.has("ddi=E"));
}

#[test]
fn a_hdmi_pll_line_names_its_mode_and_an_off_pll_prints_nothing() {
    let mut bar = bar_vector_a();
    // DPLL2 enabled in HDMI mode, 148.5 MHz, locked
    bar.set(0x46040, 0x8000_0000).set(0x6c058, 0x0002_1003).set(0x6c060, 0x0001_0001);
    bar.set(0x6c048, 0x8040_0173).set(0x6c04c, 0x0000_02c5);
    let lg = run_lines(&mut bar);
    assert!(lg.has("[I915-KBL-READOUT-DPLL id=2 enabled=1 locked=1 mode=hdmi ssc=0 rate_field=0 port_khz=148500 ctrl1=0x21]"), "{:?}", lg.0);
    assert!(!lg.has("DPLL id=1") && !lg.has("DPLL id=3"));
    // a decode error in a PLL is a line, not a silent skip
    let mut bar = bar_vector_a();
    bar.set(0x46040, 0x8000_0000).set(0x6c058, 0x0002_1003).set(0x6c04c, 0x0000_000c); // PDIV field 3 (undefined)
    let lg = run_lines(&mut bar);
    assert!(lg.has("[I915-KBL-READOUT-DPLL error wrpll-pdiv-reserved dpll=2 field=0x3]"), "{:?}", lg.0);
}

#[test]
fn a_line_longer_than_the_buffer_is_cut_but_still_ends_in_a_newline() {
    use i915_device::log::{emit, Line};
    let mut raw = Vec::new();
    struct Raw<'a>(&'a mut Vec<String>);
    impl Log for Raw<'_> {
        fn line(&mut self, s: &str) {
            self.0.push(s.to_string());
        }
    }
    let long = "x".repeat(1000);
    emit(&mut Raw(&mut raw), format_args!("{}", long));
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0].len(), 320, "319 bytes of text + the newline");
    assert!(raw[0].ends_with("x\n"));
    emit(&mut Raw(&mut raw), format_args!("short"));
    assert_eq!(raw[1], "short\n");
    let l = Line::default();
    assert_eq!(l.as_str(), "");
}

#[test]
fn slots_sizes_and_exit_codes_are_the_documented_values() {
    use i915_device::regio::{BAR0_SLOT, CFG_HEADER_BYTES, CFG_SLOT};
    use i915_device::wire::{exit_code, handle, PACE_MS, REQUEST_BUF};
    assert_eq!((BAR0_SLOT, CFG_SLOT, CFG_HEADER_BYTES, READOUT_WINDOW_BYTES), (0, 1, 0x40, 0x80000));
    assert_eq!((exit_code::NO_WINDOW, exit_code::STARTUP_FAILED, exit_code::PANIC), (1, 2, 3));
    assert_eq!((REQUEST_BUF, PACE_MS), (64, 10));
    // handle(): nothing usable -> None; a request -> its answer; a count past the buffer is an error word, not a length
    let mut buf = [0u8; REQUEST_BUF];
    buf[0] = b'I';
    buf[1] = 1;
    buf[2] = 2;
    assert_eq!(handle(0, &buf), None);
    assert_eq!(handle(3, &buf), Some([b'i', 1, 2, 1]));
    assert_eq!(handle(64, &buf), Some(answer(&buf)));
    assert_eq!(handle(65, &buf), None);
    assert_eq!(handle(u64::MAX, &buf), None);
    assert_eq!(handle(1, &buf), Some([b'i', 1, 0xff, 2])); // one byte: malformed, still answered
}
