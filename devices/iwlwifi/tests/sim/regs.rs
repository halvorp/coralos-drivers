// SPDX-License-Identifier: GPL-2.0-only
//! The register file of the simulated card. Every offset is a LITERAL from the Linux headers named beside it.
use super::*;
use fwmodel as fm;

// ---- CSR (BAR0 + off), iwl-csr.h ----
pub const CSR_HW_IF_CONFIG_REG: u32 = 0x000; // iwl-csr.h:29
pub const CSR_INT_COALESCING: u32 = 0x004; // iwl-csr.h:30
pub const CSR_INT_PERIODIC_REG: u32 = 0x005; // iwl-csr.h:40
pub const CSR_INT: u32 = 0x008; // iwl-csr.h:31
pub const CSR_INT_MASK: u32 = 0x00c; // iwl-csr.h:32
pub const CSR_FH_INT_STATUS: u32 = 0x010; // iwl-csr.h:33
pub const CSR_RESET: u32 = 0x020; // iwl-csr.h:35
pub const CSR_GP_CNTRL: u32 = 0x024; // iwl-csr.h:36
pub const CSR_HW_REV: u32 = 0x028; // iwl-csr.h:50
pub const CSR_HW_RF_ID: u32 = 0x09c; // iwl-csr.h:61
pub const CSR_MAC_ADDR0_OTP: u32 = 0x380; // cfg/9000.c:43 + iwl-csr.h:655
pub const CSR_MAC_ADDR1_OTP: u32 = 0x384;
pub const CSR_MAC_ADDR0_STRAP: u32 = 0x388;
pub const CSR_MAC_ADDR1_STRAP: u32 = 0x38c;
pub const HBUS_TARG_MEM_RADDR: u32 = 0x40c; // iwl-csr.h:522
pub const HBUS_TARG_MEM_WADDR: u32 = 0x410; // iwl-csr.h:523
pub const HBUS_TARG_MEM_WDAT: u32 = 0x418; // iwl-csr.h:524
pub const HBUS_TARG_MEM_RDAT: u32 = 0x41c; // iwl-csr.h:525
pub const HBUS_TARG_PRPH_WADDR: u32 = 0x444; // iwl-csr.h:539
pub const HBUS_TARG_PRPH_RADDR: u32 = 0x448; // iwl-csr.h:540
pub const HBUS_TARG_PRPH_WDAT: u32 = 0x44c; // iwl-csr.h:541
pub const HBUS_TARG_PRPH_RDAT: u32 = 0x450; // iwl-csr.h:542
pub const HBUS_TARG_WRPTR: u32 = 0x460; // iwl-csr.h:554

pub const GP_MAC_CLOCK_READY: u32 = 0x1; // iwl-csr.h:287
pub const GP_INIT_DONE: u32 = 0x4; // iwl-csr.h:288
pub const GP_MAC_ACCESS_REQ: u32 = 0x8; // iwl-csr.h:289
pub const GP_GOING_TO_SLEEP: u32 = 0x10; // iwl-csr.h:290
pub const GP_HW_RF_KILL_SW: u32 = 0x0800_0000; // iwl-csr.h:297
/// What an Atlas boot read before anything touched the card: `gp_cntrl=0x08040000` (the boot log, [IWL-PCI …]).
pub const GP_CNTRL_RESET_VALUE: u32 = 0x0804_0000;
pub const RESET_SW_RESET: u32 = 0x80; // iwl-csr.h:243
pub const RESET_MASTER_DISABLED: u32 = 0x100; // iwl-csr.h:244
pub const RESET_STOP_MASTER: u32 = 0x200; // iwl-csr.h:245
pub const RESET_FORCE_NMI: u32 = 0x2; // iwl-csr.h:242
pub const OWN_SET: u32 = 0x0040_0000; // iwl-csr.h:182

pub const INT_FH_RX: u32 = 1 << 31;
pub const INT_HW_ERR: u32 = 1 << 29;
pub const INT_FH_TX: u32 = 1 << 27;
pub const INT_SW_ERR: u32 = 1 << 25;
pub const INT_ALIVE: u32 = 1 << 0;

// ---- FH (flow handler), iwl-fh.h ----
pub const FH_KW_MEM_ADDR_REG: u32 = 0x197c; // iwl-fh.h:44
pub const FH_TFDIB_CTRL0_9: u32 = 0x1900 + 0x8 * 9; // iwl-fh.h:438-441
pub const FH_TFDIB_CTRL1_9: u32 = 0x1900 + 0x8 * 9 + 4;
pub const FH_SRVC_CHNL_SRAM_ADDR_9: u32 = 0x19c8; // iwl-fh.h:545-548
pub const FH_TCSR_9: u32 = 0x1d00 + 0x20 * 9; // iwl-fh.h:465-477
pub const FH_UCODE_LOAD_STATUS: u32 = 0x1af0; // iwl-prph.h:243
pub const FH_TX_CHICKEN_BITS_REG: u32 = 0x1e98; // iwl-fh.h:550
pub const FH_TSSR_TX_STATUS_REG: u32 = 0x1ea0 + 0x10; // iwl-fh.h:516-519
pub const RFH_Q0_FRBDCB_WIDX_TRG: u32 = 0x1c80; // iwl-fh.h:337
pub fn cbbc_queue(q: u32) -> u32 {
    // iwl-fh.h:61-66
    match q {
        0..=15 => 0x1000 + 0x9d0 + 4 * q,
        16..=19 => 0x1000 + 0xbf0 + 4 * (q - 16),
        _ => 0x1000 + 0xb20 + 4 * (q - 20),
    }
}

// ---- PRPH (masked to 20 bits), iwl-prph.h / iwl-fh.h ----
pub const P_RELEASE_CPU_RESET: u32 = 0x300c; // iwl-prph.h:84
pub const P_LMPM_CHICK: u32 = 0xa01ff8 & 0xfffff; // iwl-prph.h:422
pub const P_WFPM_GP2: u32 = 0xa030b4 & 0xfffff; // iwl-prph.h:359
pub const P_SCD_SRAM_BASE: u32 = 0xa02c00 & 0xfffff; // iwl-prph.h:223-225
pub const P_SCD_TXFACT: u32 = P_SCD_SRAM_BASE + 0x10;
pub const P_SCD_EN_CTRL: u32 = P_SCD_SRAM_BASE + 0x254;
pub const P_RFH_GEN_CFG: u32 = 0xa09800 & 0xfffff; // iwl-fh.h:424
pub const P_RFH_RXQ_ACTIVE: u32 = 0xa0980c & 0xfffff; // iwl-fh.h:422
pub const P_RFH_GEN_STATUS: u32 = 0xa09808 & 0xfffff; // iwl-fh.h:380
pub const P_RFH_DMA_CFG: u32 = 0xa09820 & 0xfffff; // iwl-fh.h:387
pub const P_FRBDCB_BA: u32 = 0xa08000 & 0xfffff; // iwl-fh.h:331
pub const P_FRBDCB_WIDX: u32 = 0xa08080 & 0xfffff;
pub const P_FRBDCB_RIDX: u32 = 0xa080c0 & 0xfffff;
pub const P_URBDCB_BA: u32 = 0xa08100 & 0xfffff;
pub const P_URBDCB_WIDX: u32 = 0xa08180 & 0xfffff;
pub const P_URBD_STTS: u32 = 0xa08200 & 0xfffff;

fn is_fh(off: u32) -> bool {
    (0x1000..0x2000).contains(&off)
}

/// HPM_DEBUG (0xA03440) and PREG_PRPH_WPROT_9000 (0xA04CE0) are WFPM registers Linux touches with the `_no_grab`
/// accessors BEFORE INIT_DONE, with the MAC asleep (`iwl_trans_pcie_clear_persistence_bit`, trans.c:1749-1759).
/// The sim lets exactly those through; every other periphery access needs the MAC awake.
fn wfpm_without_mac(a: u32) -> bool {
    matches!(a & 0xfffff, 0x3440 | 0x4CE0)
}

pub fn read32(w: &mut World, off: u32) -> u32 {
    if is_fh(off) && !(0x1c80..0x1d00).contains(&off) && !w.awake() {
        w.violation(format!("FH read {off:#x} without MAC access"));
    }
    match off {
        CSR_INT => w.inta,
        CSR_INT_MASK => w.int_mask,
        CSR_FH_INT_STATUS => w.fh_int,
        CSR_GP_CNTRL => {
            // MAC_CLOCK_READY follows INIT_DONE ONLY: "After device reset, this bit remains 0 until host sets
            // INIT_DONE" (iwl-csr.h:277-278), and an access request alone does not raise it (silicon, Z8350-class
            // Atlas 2026-10-16: gp_cntrl 0x08040008 = ACCESS_REQ without INIT_DONE never got a clock).
            // GOING_TO_SLEEP never shows while awake.
            let mut v = w.gp_cntrl & !(GP_MAC_CLOCK_READY | GP_GOING_TO_SLEEP | GP_HW_RF_KILL_SW);
            if !w.hw_rfkill {
                v |= GP_HW_RF_KILL_SW; // the switch is a pin: a card reset does not move it
            }
            if !w.wedge_mac_access && v & GP_INIT_DONE != 0 {
                v |= GP_MAC_CLOCK_READY;
            }
            v
        }
        CSR_RESET => *w.csr.get(&CSR_RESET).unwrap_or(&0),
        HBUS_TARG_PRPH_RDAT => {
            if !w.awake() && !wfpm_without_mac(w.prph_raddr) {
                w.violation(format!("PRPH read {:#x} without MAC access", w.prph_raddr));
            }
            let a = w.prph_raddr;
            prph_read(w, a)
        }
        HBUS_TARG_MEM_RDAT => {
            if !w.awake() {
                w.violation(format!("SRAM read {:#x} without MAC access", w.mem_raddr));
            }
            let mut b = [0u8; 4];
            let a = w.mem_raddr;
            w.sram_read(a, &mut b);
            w.mem_raddr += 4;
            u32::from_le_bytes(b)
        }
        FH_TSSR_TX_STATUS_REG => 0x00FF_0000, // every TX channel idle (iwl-fh.h:541)
        _ if is_fh(off) => *w.fh.get(&off).unwrap_or(&0),
        _ => *w.csr.get(&off).unwrap_or(&0),
    }
}

fn prph_read(w: &mut World, addr: u32) -> u32 {
    match addr {
        P_SCD_SRAM_BASE => w.fw.scd_base_reg,
        P_RFH_GEN_STATUS => 0x8000_0000, // RXF_DMA_IDLE = BIT(31), iwl-fh.h:384
        _ => *w.prph.get(&addr).unwrap_or(&0),
    }
}

pub fn write8(w: &mut World, off: u32, val: u8) {
    w.writes.push((w.now_us, "w8", off, val as u32));
    w.csr.insert(off, val as u32);
}

pub fn write32(w: &mut World, off: u32, val: u32) {
    w.writes.push((w.now_us, "w32", off, val));
    if is_fh(off) && !(0x1c80..0x1d00).contains(&off) && !w.awake() {
        w.violation(format!("FH write {off:#x} <- {val:#x} without MAC access"));
    }
    match off {
        CSR_INT => w.inta &= !val,
        CSR_FH_INT_STATUS => w.fh_int &= !val,
        CSR_INT_MASK => {
            w.int_mask = val;
            if w.inta & val != 0 {
                w.irq_pending = true; // a cause already pending is announced when it is unmasked
            }
        }
        CSR_GP_CNTRL => w.gp_cntrl = val,
        CSR_RESET => {
            if val & RESET_FORCE_NMI != 0 {
                w.forced_nmi += 1;
            }
            let mut v = val & !(RESET_SW_RESET | RESET_MASTER_DISABLED);
            if val & RESET_STOP_MASTER != 0 {
                v |= RESET_MASTER_DISABLED;
            }
            w.csr.insert(CSR_RESET, v);
            if val & RESET_SW_RESET != 0 {
                reset(w);
            }
        }
        HBUS_TARG_PRPH_WADDR => w.prph_waddr = val & 0xfffff,
        HBUS_TARG_PRPH_RADDR => w.prph_raddr = val & 0xfffff,
        HBUS_TARG_PRPH_WDAT => {
            if !w.awake() && !wfpm_without_mac(w.prph_waddr) {
                w.violation(format!("PRPH write {:#x} <- {val:#x} without MAC access", w.prph_waddr));
            }
            let a = w.prph_waddr;
            prph_write(w, a, val);
        }
        HBUS_TARG_MEM_RADDR => w.mem_raddr = val,
        HBUS_TARG_MEM_WADDR => w.mem_waddr = val,
        HBUS_TARG_MEM_WDAT => {
            if !w.awake() {
                w.violation(format!("SRAM write {:#x} without MAC access", w.mem_waddr));
            }
            let a = w.mem_waddr;
            w.sram_write(a, &val.to_le_bytes());
            w.mem_waddr += 4;
        }
        HBUS_TARG_WRPTR => {
            let q = (val >> 8) & 0x3f;
            if q == 0 {
                fm::doorbell(w, (val & 0xff) as u16);
            } else {
                w.violation(format!("doorbell for queue {q}: no data queue is enabled"));
            }
        }
        RFH_Q0_FRBDCB_WIDX_TRG => {
            w.rx_free_widx = val as u16;
        }
        FH_UCODE_LOAD_STATUS => {
            w.fh.insert(off, val);
            w.load_status_history.push(val);
            fm::load_status_written(w, val);
        }
        FH_TCSR_9 => {
            w.fh.insert(off, val);
            if val & 0x8000_0000 != 0 {
                service_channel_go(w);
            }
        }
        _ if is_fh(off) => {
            w.fh.insert(off, val);
        }
        _ => {
            w.csr.insert(off, val);
        }
    }
}

fn prph_write(w: &mut World, addr: u32, val: u32) {
    w.writes.push((w.now_us, "prph", addr, val));
    w.prph.insert(addr, val);
    match addr {
        P_RELEASE_CPU_RESET => {
            if val & (1 << 24) != 0 {
                w.fw.cpu_released = true;
            }
        }
        P_RFH_DMA_CFG | P_RFH_RXQ_ACTIVE | P_FRBDCB_BA | P_URBDCB_BA | P_URBD_STTS => {}
        P_FRBDCB_WIDX | P_FRBDCB_RIDX | P_URBDCB_WIDX => {
            // reset the device index tables (rx.c:946-948)
            w.rx_free_ridx = 0;
            w.rx_used_widx = 0;
            w.rx_free_widx = 0;
        }
        _ => {}
    }
}

/// The service channel (FH channel 9) was enabled: move one chunk host memory -> SRAM. // iwl-fh.h:544-548
fn service_channel_go(w: &mut World) {
    let ctrl0 = *w.fh.get(&FH_TFDIB_CTRL0_9).unwrap_or(&0);
    let ctrl1 = *w.fh.get(&FH_TFDIB_CTRL1_9).unwrap_or(&0);
    let sram = *w.fh.get(&FH_SRVC_CHNL_SRAM_ADDR_9).unwrap_or(&0);
    let sts = *w.fh.get(&(FH_TCSR_9 + 8)).unwrap_or(&0);
    let len = (ctrl1 & 0x0fff_ffff) as usize;
    let phys = ctrl0 as u64 | (((ctrl1 >> 28) & 0xf) as u64) << 32;
    if sts & 0x3 != 0x3 || sts & (1 << 20) == 0 || sts & (1 << 12) == 0 {
        w.violation(format!("service channel enabled with BUF_STS {sts:#x}: no valid TB"));
    }
    if len == 0 || len > 0x20000 {
        w.violation(format!("service channel chunk of {len} bytes"));
        return;
    }
    if (0x40000..=0x57fff).contains(&sram) && w.prph.get(&P_LMPM_CHICK).copied().unwrap_or(0) & 1 == 0 {
        w.violation(format!("chunk to {sram:#x} (extended window) without LMPM_CHICK bit 0"));
    }
    let mut buf = vec![0u8; len];
    if !w.dma_read(phys, &mut buf) {
        w.violation(format!("service channel DMA read of {len} bytes at {phys:#x}: not host memory"));
        return;
    }
    w.sram_write(sram, &buf);
    w.srvc_chunks.push((sram, phys, len as u32));
    w.schedule(200, Ev::ChunkDone);
}

/// CSR_RESET.SW_RESET: the card forgets everything the host programmed.
fn reset(w: &mut World) {
    w.inta = 0;
    w.int_mask = 0;
    w.fh_int = 0;
    w.gp_cntrl = GP_CNTRL_RESET_VALUE;
    w.prph.clear();
    w.fh.clear();
    w.csr.remove(&CSR_HW_IF_CONFIG_REG);
    w.irq_pending = false;
    w.rx_free_ridx = 0;
    w.rx_free_widx = 0;
    w.rx_used_widx = 0;
    w.cmd_seen_wptr = 0;
    w.fw.reset();
}
