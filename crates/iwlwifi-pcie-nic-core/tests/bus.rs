// SPDX-License-Identifier: GPL-2.0-only
//! The NIC-access handshake and the indirect accessors, over a scripted BAR0. Linux `trans.c:1881-1905, :2330-2456`,
//! `iwl-trans.c:464-481`, `iwl-io.c`.
mod common;
use common::Bus;
use iwlwifi_pcie_nic_core::bus::*;
use iwlwifi_pcie_trans_core::io::TransIo;

fn nic() -> Nic<Bus> {
    Nic::new(Bus::default())
}

#[test]
fn a_periphery_read_grabs_addresses_reads_and_releases_in_that_order() {
    let mut n = nic();
    n.bus.regs.insert(0x1_0000 | 0x2c00, 0xdead_beef); // what PRPH_RDAT returns for address 0x2c00
    let v = n.prph_read(0xA0_2C00); // SCD_SRAM_BASE_ADDR: the address is masked to 20 bits (trans.c:1891-1893)
    assert_eq!(v, 0xdead_beef);
    // GP_CNTRL |= ACCESS_REQ; RADDR <- (addr & 0xFFFFF) | 3 << 24; ...; GP_CNTRL &= ~ACCESS_REQ
    let w32: Vec<(u32, u32)> = n.bus.log.iter().filter(|l| l.0 == "w32").map(|l| (l.1, l.2)).collect();
    assert_eq!(w32, [(0x24, 0x8), (0x448, 0x0300_2c00), (0x24, 0x1)]);
    assert_eq!(n.bus.slept_us, 2, "the 2 us settle after the request (trans.c:2355-2356); the poll hit at once");
    assert_eq!(n.grabs_held(), 0);
}

#[test]
fn a_periphery_write_is_waddr_then_wdat_inside_one_grab() {
    let mut n = nic();
    n.prph_write(0x300c, 1 << 24);
    let w32: Vec<(u32, u32)> = n.bus.log.iter().map(|l| (l.1, l.2)).collect();
    assert_eq!(w32, [(0x24, 0x8), (0x444, 0x0300_300c), (0x44c, 1 << 24), (0x24, 0x1)]); // trans.c:1902-1904
}

#[test]
fn a_64_bit_periphery_write_is_low_dword_then_high_dword_and_does_not_grab_for_itself() {
    let mut n = nic();
    assert!(n.grab());
    n.write_prph64_no_grab(0xA0_8000, 0x0000_0002_dead_b000);
    n.release();
    let w32: Vec<(u32, u32)> = n.bus.log.iter().map(|l| (l.1, l.2)).collect();
    assert_eq!(w32, [(0x24, 0x8), (0x444, 0x0300_8000), (0x44c, 0xdead_b000), (0x444, 0x0300_8004), (0x44c, 0x2), (0x24, 0x1)]); // iwl-io.c:127-128
}

#[test]
fn grabs_nest_and_only_the_outermost_release_clears_the_request() {
    let mut n = nic();
    assert!(n.grab());
    assert!(n.grab());
    n.prph_write(0x1ff8, 1); // grabs for itself inside the held grab: no second handshake
    n.release();
    assert_eq!(n.grabs_held(), 1);
    assert!(!n.bus.log.iter().any(|l| l.1 == 0x24 && l.2 & 0x8 == 0), "still requested");
    n.release();
    assert_eq!(n.grabs_held(), 0);
    let gp: Vec<u32> = n.bus.log.iter().filter(|l| l.1 == 0x24).map(|l| l.2).collect();
    assert_eq!(gp, [0x8, 0x1], "one request, one release (the scripted bus reads MAC_CLOCK_READY back in, bit 0)");
}

#[test]
fn a_card_that_does_not_wake_is_nmid_once_after_15_ms_and_every_later_grab_is_refused_at_once() {
    let mut n = nic();
    n.bus.wedge = true;
    assert_eq!(n.prph_read(0x300c), READ_FAILED);
    assert!(matches!(n.fault(), Some(NicFault::AccessTimeout { .. })));
    assert!(n.bus.slept_us >= 15_000, "polled for the full 15 ms in 10 us steps (iwl-io.c:48)");
    assert!(n.bus.log.contains(&("w32", 0x20, 0x2)), "CSR_RESET <- FORCE_NMI (trans.c:2397-2398)");
    let (slept, writes) = (n.bus.slept_us, n.bus.log.len());
    assert!(!n.grab());
    assert_eq!(n.read_direct32(0x1af0), READ_FAILED);
    n.write_direct32(0x1af0, 5);
    assert_eq!((n.bus.slept_us, n.bus.log.len()), (slept, writes), "STATUS_TRANS_DEAD: nothing more is tried");
    n.clear_fault();
    n.bus.wedge = false;
    assert!(n.grab());
}

#[test]
fn flow_handler_registers_are_written_inside_a_grab_but_the_csr_block_is_not() {
    let mut n = nic();
    n.csr_write(0x1af0, 0xffff); // FH_UCODE_LOAD_STATUS: iwl_write_direct32
    assert_eq!(n.bus.log.iter().filter(|l| l.1 == 0x24).count(), 2, "grab and release around it");
    n.bus.log.clear();
    n.csr_write(0x1c80, 128); // the RFH shadow trigger: a plain iwl_write32 (rx.c:199-201)
    n.csr_write(0x008, 0xffff_ffff); // CSR_INT
    n.csr_write(0x460, 0x1); // HBUS_TARG_WRPTR
    assert_eq!(n.bus.log.len(), 3);
    assert!(n.bus.log.iter().all(|l| l.1 != 0x24));
}

#[test]
fn sram_writes_set_the_address_once_and_stream_dwords() {
    let mut n = nic();
    n.write_mem(0x0040_3800 + 0x600, &[1, 2, 3]).unwrap();
    let w32: Vec<(u32, u32)> = n.bus.log.iter().map(|l| (l.1, l.2)).collect();
    assert_eq!(w32, [(0x24, 0x8), (0x410, 0x0040_3e00), (0x418, 1), (0x418, 2), (0x418, 3), (0x24, 0x1)]); // iwl-trans.c:473-476
    let mut n = nic();
    n.zero_mem(0x100, 2).unwrap();
    let w32: Vec<(u32, u32)> = n.bus.log.iter().map(|l| (l.1, l.2)).collect();
    assert_eq!(w32, [(0x24, 0x8), (0x410, 0x100), (0x418, 0), (0x418, 0), (0x24, 0x1)], "a NULL buffer writes zeroes (iwl-trans.c:476)");
}

#[test]
fn sram_reads_set_the_read_address_once_and_stream_dwords() {
    let mut n = nic();
    let mut out = [0u32; 3];
    n.read_mem(0x2000, &mut out).unwrap();
    assert_eq!(out, [0x1000_2000, 0x1000_2004, 0x1000_2008]); // RADDR once, RDAT auto-increments (trans.c:2473-2477)
    assert_eq!(n.bus.log.iter().filter(|l| l.1 == 0x40c).count(), 1);
}

#[test]
fn a_failed_grab_fails_sram_access_by_name() {
    let mut n = nic();
    n.bus.wedge = true;
    assert!(matches!(n.write_mem(0, &[1]), Err(NicFault::AccessTimeout { .. })));
    assert!(matches!(n.read_mem(0, &mut [0]), Err(NicFault::AccessTimeout { .. })));
    assert!(matches!(n.zero_mem(0, 1), Err(NicFault::AccessTimeout { .. })));
}
