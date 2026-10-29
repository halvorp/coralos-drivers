// SPDX-License-Identifier: GPL-2.0-only
//! The cAVS interrupt that never arrives (Atlas, 2026-10-29: the function's MSI capability was never programmed, so the code loader's
//! completion of the base firmware timed out - `[AVS-DEV FAIL step=cldma-ioc stage=base-firmware]` - and the DSP, the SSP and the
//! amplifiers never ran). Each case is shown failing first: with the delivery missing and nothing measuring it, the start fails at
//! `cldma-ioc`; with the device measuring it, the same hardware boots and plays.
mod sim;
use avs_device::irqmode::{IrqMode, IrqProbe, PollWhy};
use avs_device::machine::Wake;
use avs_wire::msg::*;
use avs_wire::*;
use sim::harness::*;

type Dev = avs_device::Device<sim::Sim>;

/// A configuration image with the MSI capability at 0x60 (the Atlas's cAVS function: `caps=pm@0x50,msi@0x60`, the boot's CAPTURE-PCI
/// line), the list starting at the power-management capability at 0x50.
fn cfg_with_msi(flags: u16) -> [u8; 256] {
    let mut c = [0u8; 256];
    c[0..4].copy_from_slice(&0x9d71_8086u32.to_le_bytes());
    c[0x04..0x06].copy_from_slice(&0x0006u16.to_le_bytes()); // COMMAND as the firmware left it: MEM | BME
    c[0x06..0x08].copy_from_slice(&0x0010u16.to_le_bytes()); // STATUS: CAP_LIST (pci_regs.h:56)
    c[0x34] = 0x50; // PCI_CAPABILITY_LIST (pci_regs.h:122)
    c[0x50] = 0x01; // PCI_CAP_ID_PM (pci_regs.h:222)
    c[0x51] = 0x60;
    c[0x60] = 0x05; // PCI_CAP_ID_MSI (pci_regs.h:223)
    c[0x61] = 0x00;
    c[0x62..0x64].copy_from_slice(&flags.to_le_bytes());
    c
}

fn logged(dev: &Dev, prefix: &str) -> Vec<String> {
    dev.rig.m.logs.iter().filter(|l| l.starts_with(prefix)).map(|l| l.trim_end().to_string()).collect()
}

fn speaker_dev(cfg: Option<[u8; 256]>) -> Option<Dev> {
    let mut dev = started_with_speakers(false)?;
    if let Some(c) = cfg {
        dev.rig.m.cfg = c;
    }
    dev.rig.m.irq_undelivered = true;
    Some(dev)
}

fn start_speakers(dev: &mut Dev, c: &mut Client) -> (Status, PhaseReply) {
    let id = open_stream_route(dev, c, Route::Speakers);
    for chunk in ramp(8192, 1).chunks(16384).take(2) {
        call(dev, c, Opcode::PcmWrite, &pcm_payload(id, chunk));
    }
    start_stream(dev, c, id)
}

#[test]
fn the_probe_reads_the_msi_capability_and_polls_only_when_it_is_off_or_there_is_no_line() {
    // PCI_MSI_FLAGS_ENABLE = 0x0001, PCI_MSI_FLAGS_64BIT = 0x0080 (pci_regs.h:316, :319), typed by hand
    assert_eq!(avs_device::irqmode::PCI_CAP_ID_MSI, 0x05);
    assert_eq!(avs_device::irqmode::PCI_MSI_FLAGS, 0x02);
    assert_eq!(avs_device::irqmode::PCI_MSI_FLAGS_ENABLE, 0x0001);
    assert_eq!(avs_device::irqmode::PCI_MSI_FLAGS_64BIT, 0x0080);
    assert_eq!((avs_device::irqmode::PCI_MSI_ADDRESS_LO, avs_device::irqmode::PCI_MSI_DATA_32, avs_device::irqmode::PCI_MSI_DATA_64), (0x04, 0x08, 0x0c));
    let off = IrqProbe::decode(&cfg_with_msi(0x0080), true);
    assert_eq!((off.msi_cap, off.msi_flags, off.command, off.status), (Some(0x60), 0x0080, 0x0006, 0x0010));
    assert_eq!(off.mode(), IrqMode::Poll(PollWhy::MsiDisabled));
    // enabled, 64-bit: the data word is at +0x0c
    let mut c = cfg_with_msi(0x0081);
    c[0x64..0x68].copy_from_slice(&0xfee0_0000u32.to_le_bytes());
    c[0x6c..0x6e].copy_from_slice(&0x0042u16.to_le_bytes());
    let on = IrqProbe::decode(&c, true);
    assert_eq!((on.msi_addr_lo, on.msi_data, on.mode()), (0xfee0_0000, 0x0042, IrqMode::Line));
    // 32-bit: the data word is at +0x08
    let mut c = cfg_with_msi(0x0001);
    c[0x68..0x6a].copy_from_slice(&0x0031u16.to_le_bytes());
    assert_eq!(IrqProbe::decode(&c, true).msi_data, 0x0031);
    // no line granted: poll whatever the capability says
    assert_eq!(IrqProbe::decode(&cfg_with_msi(0x0081), false).mode(), IrqMode::Poll(PollWhy::NoLine));
    // no capability list (the sim's default image): trust the line
    let mut bare = [0u8; 256];
    bare[0..4].copy_from_slice(&0x9d71_8086u32.to_le_bytes());
    let p = IrqProbe::decode(&bare, true);
    assert_eq!((p.msi_cap, p.mode()), (None, IrqMode::Line));
    assert_eq!((PollWhy::MsiDisabled.name(), PollWhy::NoLine.name()), ("msi-disabled", "no-line"));
}

#[test]
fn an_msi_that_was_never_enabled_is_measured_and_the_speakers_boot_play_and_report_by_polling() {
    let Some(mut dev) = speaker_dev(Some(cfg_with_msi(0x0080))) else { return };
    let mut c = Client::new();
    let (s, ph) = start_speakers(&mut dev, &mut c);
    assert_eq!((s, ph.phase, ph.cause), (Status::Ok, StreamPhase::Running, Cause::None), "{:?}", logged(&dev, "[AVS-"));
    let irq = logged(&dev, "[AVS-DEV IRQ mode=");
    assert_eq!(irq.len(), 1, "{irq:?}");
    assert!(irq[0].starts_with("[AVS-DEV IRQ mode=poll why=msi-disabled line=true msi-cap=0x60 msi-flags=0x0080"), "{}", irq[0]);
    assert!(logged(&dev, "[AVS-DEV SESSION up").iter().any(|l| l.contains("irq=poll lost=0")));
    assert!(logged(&dev, "[AVS-DEV IRQ-LOST").is_empty(), "poll mode was chosen up front: no wait had to time out first");
    run_ms(&mut dev, 1100);
    // no interrupt ever woke the task, and yet the stream ran: the periods were found by looking
    assert!(dev.rig.m.waits.iter().all(|w| w.woke != Wake::AvsIrq));
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.output), (State::Running, Output::Speakers), "{:?}", logged(&dev, "[AVS-DEV"));
    let played = dev.rig.m.played(7);
    assert!(played.iter().any(|b| *b != 0), "the client's samples reached the speakers' link");
    // the ring says what the hardware did
    for side in ["Right", "Left"] {
        assert!(logged(&dev, "[AVS-SPK BRINGUP").iter().any(|l| l.contains(&format!("side={side}")) && l.ends_with("ok]")), "{side}");
        assert!(logged(&dev, "[AVS-SPK PROTECT").iter().any(|l| l.contains(&format!("side={side}")) && l.ends_with("ok]")), "{side}");
    }
    assert_eq!(logged(&dev, "[AVS-SPK ENABLE").len(), 2);
    let pos: Vec<_> = logged(&dev, "[AVS-DEV POS");
    assert!(pos[0].starts_with("[AVS-DEV POS t=100ms output=speakers"), "{pos:?}");
    assert!(pos[1].starts_with("[AVS-DEV POS t=1000ms output=speakers"), "{pos:?}");
    assert!(pos.iter().all(|l| l.contains("poll=true") && !l.contains("hw_bytes=0 ")), "the link position moved: {pos:?}");
    let spk = logged(&dev, "[AVS-SPK STATUS t=1000ms");
    assert_eq!(spk.len(), 2, "one line per amplifier: {spk:?}");
    assert!(spk.iter().all(|l| l.contains("shdn=01")), "GLOBAL_EN reads back on: {spk:?}");
    let ppl = logged(&dev, "[AVS-DEV PPL t=1000ms");
    assert_eq!(ppl.len(), 1, "{:?}", logged(&dev, "[AVS-"));
    assert!(ppl[0].contains("=running") && !ppl[0].contains("ipc-failed"), "{}", ppl[0]);
}

#[test]
fn a_line_believed_but_never_delivered_is_caught_by_the_last_look_and_a_transfer_that_never_completes_fails_with_a_snapshot() {
    // FAILING FIRST: the device believes the line (no MSI capability in the image) and the interrupt never comes. The wait's last look
    // finds the completion pending, names the lost interrupt and goes on polling - the boot survives it.
    let Some(mut dev) = speaker_dev(None) else { return };
    let mut c = Client::new();
    let (s, ph) = start_speakers(&mut dev, &mut c);
    assert_eq!((s, ph.phase), (Status::Ok, StreamPhase::Running), "{:?}", logged(&dev, "[AVS-"));
    assert!(logged(&dev, "[AVS-DEV IRQ mode=line").len() == 1);
    let lost = logged(&dev, "[AVS-DEV IRQ-LOST");
    assert_eq!(lost.len(), 1, "named once, then the device polls: {lost:?}");
    assert!(lost[0].contains("lost=1 -> mode=poll"));
    assert!(dev.rig.irq_poll && dev.rig.lost_irqs == 1);
    // and a transfer that never completes at all (no cause for any look to find) fails exactly where the Atlas failed, by name, with
    // the registers that tell the two apart on the ring
    let Some(mut dev) = speaker_dev(None) else { return };
    dev.rig.irq_poll = false;
    dev.rig.m.dsp.cl_never_signals = true;
    let mut c = Client::new();
    let (s, _) = start_speakers(&mut dev, &mut c);
    assert_eq!(s, Status::InitFailed);
    let st = get_status(&mut dev, &mut c);
    assert_eq!(st.step_str(), "cldma-ioc");
    let snap = logged(&dev, "[AVS-DEV IRQ-SNAPSHOT step=cldma-ioc");
    assert_eq!(snap.len(), 1, "{:?}", logged(&dev, "[AVS-"));
    assert!(snap[0].contains("poll=false"), "{}", snap[0]);
}

#[test]
fn a_polled_speaker_stream_keeps_feeding_the_clients_pcm_past_one_ring_and_never_trips_the_period_watchdog() {
    // Atlas 2026-10-29 ("azure pine dune", drivers c1fa296): the DSP booted in poll mode, the link ran ONE ring (`lpib=0x4000`,
    // `hw_bytes=15744`), `pcm_fed=0`, then `[AVS-DEV FAULT step=period-timeout]`. The host stream had SPIB armed at the ring's size, so the
    // engine fetched the buffer once and stalled at its end; no period completed after that, nothing of the client's PCM was ever moved.
    let Some(mut dev) = speaker_dev(Some(cfg_with_msi(0x0080))) else { return };
    let mut c = Client::new();
    let (s, ph) = start_speakers(&mut dev, &mut c);
    assert_eq!((s, ph.phase), (Status::Ok, StreamPhase::Running), "{:?}", logged(&dev, "[AVS-"));
    let id = dev.stream.as_ref().unwrap().id;
    // a client that keeps the FIFO topped up, as Aura does on every EV-PERIOD: 4096 bytes (one period) every 20 ms for 1.5 s
    let mut n = 0u64;
    for _ in 0..75 {
        call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(1024, 1 + n as usize * 1024)));
        n += 1;
        run_ms(&mut dev, 20);
    }
    let faults = logged(&dev, "[AVS-DEV FAULT");
    assert!(faults.is_empty(), "{faults:?}");
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.output), (State::Running, Output::Speakers), "{:?}", logged(&dev, "[AVS-DEV"));
    let pos = logged(&dev, "[AVS-DEV POS");
    let fed = |l: &str| l.split(" pcm_fed=").nth(1).and_then(|r| r.split(' ').next()).and_then(|v| v.parse::<u64>().ok()).unwrap();
    assert_eq!(pos.len(), 2, "{pos:?}");
    assert!(fed(&pos[1]) > fed(&pos[0]) && fed(&pos[1]) > 16384, "the client's PCM keeps being fed past one ring: {pos:?}");
    // the engine read far more than one ring (16 KiB) - 1.5 s at 192 B/ms
    assert!(dev.rig.m.played(7).len() > 4 * 16384, "the link stalled after {} bytes", dev.rig.m.played(7).len());
    assert!(dev.dsp.as_ref().unwrap().acct.pcm_fed > 64 * 1024);
}

#[test]
fn in_poll_mode_a_completion_bit_nobody_sees_is_served_from_the_link_position_and_a_dma_that_stops_is_named_dma_stalled() {
    // (a) the engine raises no completion bit at all: the poll serves every period from LPIB, the stream runs, nothing faults
    let Some(mut dev) = speaker_dev(Some(cfg_with_msi(0x0080))) else { return };
    let mut c = Client::new();
    let (s, _) = start_speakers(&mut dev, &mut c);
    assert_eq!(s, Status::Ok);
    dev.rig.m.hda.mute_irqs = true;
    run_ms(&mut dev, 600);
    assert!(logged(&dev, "[AVS-DEV FAULT").is_empty(), "{:?}", logged(&dev, "[AVS-DEV FAULT"));
    assert!(dev.dsp.as_ref().unwrap().lpib_periods >= 20, "periods served from the position: {}", dev.dsp.as_ref().unwrap().lpib_periods);
    // (b) the DMA stops (its RUN bit dropped behind the driver's back): the watchdog names it by the position that did not move
    let Some(mut dev) = speaker_dev(Some(cfg_with_msi(0x0080))) else { return };
    let mut c = Client::new();
    let (s, _) = start_speakers(&mut dev, &mut c);
    assert_eq!(s, Status::Ok);
    let ctl = sim::hda::sd(7) as usize;
    dev.rig.m.hda.mem[ctl] &= !0x02;
    run_ms(&mut dev, 600);
    let f = logged(&dev, "[AVS-DEV FAULT");
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].starts_with("[AVS-DEV FAULT step=dma-stalled"), "{}", f[0]);
}
