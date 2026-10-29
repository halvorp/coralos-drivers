// SPDX-License-Identifier: GPL-2.0-only
//! The cAVS interrupt. It is the function's legacy INTx line - INTA of 00:1f.3, GSI 16 level-low on the Pixelbook Go, SHARED with the other PCH
//! functions that route INTA to 16 - granted to the device as a platform line in IRQ slot 0. ChromeOS's own snd_soc_avs never uses MSI on this part
//! (`avs_pci_probe`: `PCI_IRQ_LEGACY`, plus `PCI_IRQ_MSI` only without `AVS_PLATATTR_CLDMA`, which `skl_desc` has), so an MSI capability that is
//! present and OFF is the expected state, not a reason to poll.
//!
//! History (Atlas, 2026-10-29): the kernel allocated an MSI route nobody programmed, the device polled (`mode=poll why=msi-disabled`) and the
//! vector 0x42 was offered to IRQ_ARM as if it were GSI 66. Each case is shown failing first. Polling stays as the NAMED fallback: no line granted
//! (`why=no-line`), the function's MSI turned on behind the device's back (`why=msi-enabled`), INTx disabled in COMMAND (`why=intx-disabled`), an
//! interrupt that never arrived (`IRQ-LOST`), and a shared line some other function holds asserted (`IRQ-STORM`, the line left masked).
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

/// How the cAVS interrupt reaches (or does not reach) the device in a test.
#[derive(Clone, Copy)]
enum Line {
    /// The INTx line is granted and delivered.
    Delivered,
    /// The line is granted but the interrupt never arrives (the causes are raised in the registers only).
    Undelivered,
    /// The endowment carried no line.
    NotGranted,
}

fn speaker_dev(cfg: Option<[u8; 256]>, line: Line) -> Option<Dev> {
    let mut dev = started_with_speakers(false)?;
    if let Some(c) = cfg {
        dev.rig.m.cfg = c;
    }
    dev.rig.m.irq_undelivered = matches!(line, Line::Undelivered);
    dev.rig.m.line_granted = !matches!(line, Line::NotGranted);
    Some(dev)
}

/// The Atlas's own configuration image: the MSI capability present and OFF (`msi-flags=0x0080`, the boot's CAPTURE-PCI line).
fn atlas_cfg() -> [u8; 256] {
    cfg_with_msi(0x0080)
}

fn start_speakers(dev: &mut Dev, c: &mut Client) -> (Status, PhaseReply) {
    let id = open_stream_route(dev, c, Route::Speakers);
    for chunk in ramp(8192, 1).chunks(16384).take(2) {
        call(dev, c, Opcode::PcmWrite, &pcm_payload(id, chunk));
    }
    start_stream(dev, c, id)
}

#[test]
fn the_probe_takes_the_intx_line_when_the_msi_is_off_and_polls_by_name_only_when_the_line_cannot_carry_the_interrupt() {
    // PCI_MSI_FLAGS_ENABLE = 0x0001, PCI_MSI_FLAGS_64BIT = 0x0080 (pci_regs.h:316, :319), PCI_COMMAND_INTX_DISABLE = 0x400 (pci_regs.h:51), typed by hand
    assert_eq!(avs_device::irqmode::PCI_CAP_ID_MSI, 0x05);
    assert_eq!(avs_device::irqmode::PCI_MSI_FLAGS, 0x02);
    assert_eq!(avs_device::irqmode::PCI_MSI_FLAGS_ENABLE, 0x0001);
    assert_eq!(avs_device::irqmode::PCI_MSI_FLAGS_64BIT, 0x0080);
    assert_eq!(avs_device::irqmode::PCI_COMMAND_INTX_DISABLE, 0x0400);
    assert_eq!((avs_device::irqmode::PCI_MSI_ADDRESS_LO, avs_device::irqmode::PCI_MSI_DATA_32, avs_device::irqmode::PCI_MSI_DATA_64), (0x04, 0x08, 0x0c));
    // the Atlas: MSI present and off, INTx enabled, the line granted -> the line
    let off = IrqProbe::decode(&atlas_cfg(), true);
    assert_eq!((off.msi_cap, off.msi_flags, off.command, off.status), (Some(0x60), 0x0080, 0x0006, 0x0010));
    assert_eq!(off.mode(), IrqMode::Line);
    // MSI turned on (64-bit, the data word at +0x0c): the function signals as MSI, which this task does not hold -> poll, by name
    let mut c = cfg_with_msi(0x0081);
    c[0x64..0x68].copy_from_slice(&0xfee0_0000u32.to_le_bytes());
    c[0x6c..0x6e].copy_from_slice(&0x0042u16.to_le_bytes());
    let on = IrqProbe::decode(&c, true);
    assert_eq!((on.msi_addr_lo, on.msi_data, on.mode()), (0xfee0_0000, 0x0042, IrqMode::Poll(PollWhy::MsiEnabled)));
    // 32-bit: the data word is at +0x08
    let mut c = cfg_with_msi(0x0001);
    c[0x68..0x6a].copy_from_slice(&0x0031u16.to_le_bytes());
    assert_eq!(IrqProbe::decode(&c, true).msi_data, 0x0031);
    // INTx disabled in COMMAND: the line can never assert -> poll, by name
    let mut c = atlas_cfg();
    c[0x04..0x06].copy_from_slice(&0x0406u16.to_le_bytes());
    assert_eq!(IrqProbe::decode(&c, true).mode(), IrqMode::Poll(PollWhy::IntxDisabled));
    // no line granted: poll whatever the capability says
    assert_eq!(IrqProbe::decode(&atlas_cfg(), false).mode(), IrqMode::Poll(PollWhy::NoLine));
    // no capability list (the sim's default image): the line
    let mut bare = [0u8; 256];
    bare[0..4].copy_from_slice(&0x9d71_8086u32.to_le_bytes());
    let p = IrqProbe::decode(&bare, true);
    assert_eq!((p.msi_cap, p.mode()), (None, IrqMode::Line));
    assert_eq!(
        (PollWhy::NoLine.name(), PollWhy::MsiEnabled.name(), PollWhy::IntxDisabled.name()),
        ("no-line", "msi-enabled", "intx-disabled")
    );
}

#[test]
fn on_the_atlas_intx_line_the_interrupt_services_the_code_loader_and_the_stream() {
    // FAILING FIRST (drivers 0c6ad47): an MSI capability that is off was read as "the interrupt cannot reach me" and the device polled.
    let Some(mut dev) = speaker_dev(Some(atlas_cfg()), Line::Delivered) else { return };
    let mut c = Client::new();
    let (s, ph) = start_speakers(&mut dev, &mut c);
    assert_eq!((s, ph.phase, ph.cause), (Status::Ok, StreamPhase::Running, Cause::None), "{:?}", logged(&dev, "[AVS-"));
    let irq = logged(&dev, "[AVS-DEV IRQ mode=");
    assert_eq!(irq.len(), 1, "{irq:?}");
    assert!(irq[0].starts_with("[AVS-DEV IRQ mode=line why=intx line=true msi-cap=0x60 msi-flags=0x0080"), "{}", irq[0]);
    // the code loader's completions (and the DSP's replies) arrived as interrupts: no wait had to look at its deadline, nothing was lost
    assert!(!dev.rig.irq_poll && dev.rig.lost_irqs == 0, "{:?}", logged(&dev, "[AVS-DEV IRQ"));
    assert!(logged(&dev, "[AVS-DEV SESSION up").iter().any(|l| l.contains("irq=line lost=0")));
    let woke_at_start = dev.rig.m.waits.iter().filter(|w| w.woke == Wake::AvsIrq).count();
    assert!(woke_at_start > 0, "the DSP boot was woken by the line");
    run_ms(&mut dev, 1100);
    // the stream's periods arrive on the line too
    let woke = dev.rig.m.waits.iter().filter(|w| w.woke == Wake::AvsIrq).count();
    assert!(woke > woke_at_start + 20, "periods woke the device: {woke_at_start} -> {woke}");
    assert!(dev.rig.m.irq_rearms as usize >= woke, "every firing of the level line is re-armed after its causes were serviced");
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.output), (State::Running, Output::Speakers), "{:?}", logged(&dev, "[AVS-DEV"));
    assert!(dev.rig.m.played(7).iter().any(|b| *b != 0), "the client's samples reached the speakers' link");
    let pos = logged(&dev, "[AVS-DEV POS");
    assert!(pos.len() == 2 && pos.iter().all(|l| l.contains("poll=false lost=0") && l.contains(" looks=0 ")), "{pos:?}");
    assert!(logged(&dev, "[AVS-DEV IRQ-LOST").is_empty() && logged(&dev, "[AVS-DEV IRQ-STORM").is_empty());
}

#[test]
fn a_shared_line_held_by_another_function_is_never_acked_as_ours_and_is_left_masked_while_the_device_polls() {
    // GSI 16 is shared (INTA of 00:1f.3 with xHCI, the GPU, HECI, SMBus, ... per the shipped firmware's _PRT / devintconfig). A wake whose
    // causes are none of ours (BAR0 INTSTS, the stream's SD_STS, BAR4 ADSPIS all clear) writes NOTHING to them and services nothing. A line
    // that keeps firing with no cause of ours is another function holding it asserted: after SPURIOUS_LIMIT in a row the device stops
    // re-arming it (masked: no storm on this core) and polls its own causes - named, and the stream keeps running.
    // FAILING FIRST (drivers 0c6ad47): the same firings were a session FAULT, `irq-storm`, and the audio went down.
    let Some(mut dev) = speaker_dev(Some(atlas_cfg()), Line::Delivered) else { return };
    let mut c = Client::new();
    let (s, _) = start_speakers(&mut dev, &mut c);
    assert_eq!(s, Status::Ok, "{:?}", logged(&dev, "[AVS-"));
    assert!(!dev.rig.irq_poll);
    run_ms(&mut dev, 50);
    // from here no cause of ours is ever raised (the engine's completion bits are muted), and the line is held asserted by someone else
    dev.rig.m.hda.mute_irqs = true;
    run_ms(&mut dev, 30);
    let (w0, r0) = (dev.rig.m.writes.len(), dev.rig.m.irq_rearms);
    dev.rig.m.force_irq = true;
    run_ms(&mut dev, 200);
    let cause_reg = |w: &&sim::WriteRec| {
        (w.space == avs_fw_core::effect::Space::Hda
            && (w.off == hda_ext_core::regs::AZX_REG_INTSTS || (w.off >= sim::hda::sd(0) && (w.off - sim::hda::sd(0)) % hda_ext_core::consts::SD_INTERVAL == hda_ext_core::regs::AZX_REG_SD_STS)))
            || (w.space == avs_fw_core::effect::Space::Dsp && w.off == avs_fw_core::regs::AVS_ADSP_REG_ADSPIS)
    };
    let acked: Vec<_> = dev.rig.m.writes[w0..].iter().filter(cause_reg).map(|w| (w.space, w.off, w.value)).collect();
    assert!(acked.is_empty(), "a wake with no cause of ours wrote a cause register: {acked:?}");
    let storm = logged(&dev, "[AVS-DEV IRQ-STORM");
    assert_eq!(storm.len(), 1, "{:?}", logged(&dev, "[AVS-"));
    assert!(storm[0].contains("spurious=64") && storm[0].ends_with("-> masked, mode=poll]"), "{}", storm[0]);
    assert!(dev.rig.irq_poll);
    let rearmed = dev.rig.m.irq_rearms - r0;
    assert_eq!(rearmed, 63, "re-armed after each of the first 63 empty firings, never after the 64th");
    // masked for good: no wait asks for the line again, and nothing re-arms it
    let r1 = dev.rig.m.irq_rearms;
    let n1 = dev.rig.m.waits.len();
    run_ms(&mut dev, 300);
    assert_eq!(dev.rig.m.irq_rearms, r1, "the line stays masked");
    assert!(dev.rig.m.waits[n1..].iter().all(|w| !w.want.avs_irq), "poll mode never waits on the line");
    assert!(logged(&dev, "[AVS-DEV FAULT").is_empty(), "{:?}", logged(&dev, "[AVS-DEV FAULT"));
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.output), (State::Running, Output::Speakers), "{:?}", logged(&dev, "[AVS-DEV"));
    assert!(dev.dsp.as_ref().unwrap().lpib_periods > 0, "the polled stream is served from the link position");
    dev.rig.m.force_irq = false;
}

#[test]
fn with_no_line_granted_the_speakers_boot_play_and_report_by_polling() {
    let Some(mut dev) = speaker_dev(Some(atlas_cfg()), Line::NotGranted) else { return };
    let mut c = Client::new();
    let (s, ph) = start_speakers(&mut dev, &mut c);
    assert_eq!((s, ph.phase, ph.cause), (Status::Ok, StreamPhase::Running, Cause::None), "{:?}", logged(&dev, "[AVS-"));
    let irq = logged(&dev, "[AVS-DEV IRQ mode=");
    assert_eq!(irq.len(), 1, "{irq:?}");
    assert!(irq[0].starts_with("[AVS-DEV IRQ mode=poll why=no-line line=false msi-cap=0x60 msi-flags=0x0080"), "{}", irq[0]);
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
    let Some(mut dev) = speaker_dev(None, Line::Undelivered) else { return };
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
    let Some(mut dev) = speaker_dev(None, Line::Undelivered) else { return };
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
    let Some(mut dev) = speaker_dev(Some(atlas_cfg()), Line::NotGranted) else { return };
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
    let Some(mut dev) = speaker_dev(Some(atlas_cfg()), Line::NotGranted) else { return };
    let mut c = Client::new();
    let (s, _) = start_speakers(&mut dev, &mut c);
    assert_eq!(s, Status::Ok);
    dev.rig.m.hda.mute_irqs = true;
    run_ms(&mut dev, 600);
    assert!(logged(&dev, "[AVS-DEV FAULT").is_empty(), "{:?}", logged(&dev, "[AVS-DEV FAULT"));
    assert!(dev.dsp.as_ref().unwrap().lpib_periods >= 20, "periods served from the position: {}", dev.dsp.as_ref().unwrap().lpib_periods);
    // (b) the DMA stops (its RUN bit dropped behind the driver's back): the watchdog names it by the position that did not move
    let Some(mut dev) = speaker_dev(Some(atlas_cfg()), Line::NotGranted) else { return };
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
