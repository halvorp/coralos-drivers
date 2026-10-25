// SPDX-License-Identifier: GPL-2.0-only
//! The firmware upload program: register writes per chunk and their order, the per-section status ladder, the CPU1/CPU2
//! split, the extended-address bracket, and every way it stops. Linux `pcie/gen1_2/trans.c:603-783, :973-1008`.
mod common;
use common::{Op, Rec};
use iwlwifi_fw_image_core::sections::{Section, CPU1_CPU2_SEPARATOR_SECTION as SEP, PAGING_SEPARATOR_SECTION as PSEP};
use iwlwifi_pcie_nic_core::bus::{NicFault, NicIo};
use iwlwifi_pcie_nic_core::load::*;
use iwlwifi_pcie_trans_core::io::TransIo;

/// Records the register ops of `Rec`, plus the host half.
struct Env {
    io: Rec,
    staged: Vec<(usize, u64)>,
    next_phys: u64,
    waits: usize,
    ints: usize,
    refuse_stage_at: Option<usize>,
    time_out_at: Option<usize>,
    ladder: Vec<u32>,
}

impl Env {
    fn new() -> Env {
        Env { io: Rec::default(), staged: vec![], next_phys: 0x3_0000_1000, waits: 0, ints: 0, refuse_stage_at: None, time_out_at: None, ladder: vec![] }
    }
}

impl TransIo for Env {
    fn csr_read(&mut self, r: u32) -> u32 {
        self.io.csr_read(r)
    }
    fn csr_write(&mut self, r: u32, v: u32) {
        self.io.csr_write(r, v)
    }
    fn prph_read(&mut self, a: u32) -> u32 {
        self.io.prph_read(a)
    }
    fn prph_write(&mut self, a: u32, v: u32) {
        self.io.prph_write(a, v)
    }
    fn sleep_us(&mut self, us: u32) {
        self.io.sleep_us(us)
    }
}
impl NicIo for Env {
    fn grab(&mut self) -> bool {
        self.io.grab()
    }
    fn release(&mut self) {
        self.io.release()
    }
    fn read_direct32(&mut self, r: u32) -> u32 {
        self.io.read_direct32(r)
    }
    fn write_direct32(&mut self, r: u32, v: u32) {
        if r == 0x1af0 {
            self.ladder.push(v);
        }
        self.io.write_direct32(r, v)
    }
    fn write8(&mut self, r: u32, v: u8) {
        self.io.write8(r, v)
    }
    fn write_prph64_no_grab(&mut self, a: u32, v: u64) {
        self.io.write_prph64_no_grab(a, v)
    }
    fn write_mem(&mut self, a: u32, w: &[u32]) -> Result<(), NicFault> {
        self.io.write_mem(a, w)
    }
    fn zero_mem(&mut self, a: u32, n: usize) -> Result<(), NicFault> {
        self.io.zero_mem(a, n)
    }
    fn read_mem(&mut self, a: u32, o: &mut [u32]) -> Result<(), NicFault> {
        self.io.read_mem(a, o)
    }
    fn fault(&self) -> Option<NicFault> {
        self.io.fault()
    }
}
impl LoadEnv for Env {
    fn stage(&mut self, chunk: &[u8]) -> Result<u64, LoadRefusal> {
        if self.refuse_stage_at == Some(self.staged.len()) {
            return Err(LoadRefusal::NoBounceBuffer);
        }
        self.io.ops.push(Op::Sleep(0xCAFE)); // marks "staged here" in the op stream
        self.staged.push((chunk.len(), self.next_phys));
        Ok(self.next_phys)
    }
    fn wait_chunk_done(&mut self, timeout_ms: u32) -> bool {
        assert_eq!(timeout_ms, 5000);
        self.waits += 1;
        self.time_out_at != Some(self.waits)
    }
    fn enable_interrupts(&mut self) {
        self.ints += 1;
        self.io.ops.push(Op::CsrW(0xE1, 0xE1)); // marks "interrupts enabled here"
    }
}

fn sec(offset: u32, len: usize) -> (u32, Vec<u8>) {
    (offset, vec![0x5a; len])
}

fn secs<'a>(v: &'a [(u32, Vec<u8>)]) -> Vec<Section<'a>> {
    v.iter().map(|(o, d)| Section { offset: *o, data: d }).collect()
}

#[test]
fn one_chunk_is_the_six_service_channel_writes_in_linux_order_inside_one_grab() {
    let mut e = Env::new();
    load_firmware_chunk_fh(&mut e, 0x0040_2000, 0x3_dead_b000, 712);
    assert_eq!(
        e.io.ops,
        [
            Op::CsrW(0x1e20, 0),                      // TX_CONFIG(9) <- DMA_CHNL_PAUSE                     trans.c:607-608
            Op::CsrW(0x19c8, 0x0040_2000),            // SRAM_ADDR(9) <- destination                        trans.c:610-611
            Op::CsrW(0x1948, 0xdead_b000),            // TFDIB_CTRL0(9) <- low 32 bits & 0xFFFFFFFF          trans.c:613-614
            Op::CsrW(0x194c, (3 << 28) | 712),        // TFDIB_CTRL1(9) <- (hi 4 bits << 28) | byte count    trans.c:616-618
            Op::CsrW(0x1e28, (1 << 20) | (1 << 12) | 3), // BUF_STS(9) <- TB_NUM | TB_IDX | VALID           trans.c:620-623
            Op::CsrW(0x1e20, 0x8000_0000 | 0x0010_0000), // TX_CONFIG(9) <- ENABLE | CREDIT_DISABLE | HOST_ENDTFD  trans.c:625-628
        ]
    );
}

#[test]
fn an_init_style_image_is_all_cpu1_with_the_ladder_one_three_seven_and_the_done_markers() {
    let img = [sec(0x0040_2000, 100), sec(0x0080_0000, 200), sec(0x0, 50)];
    let mut e = Env::new();
    let r = load_given_ucode_8000(&mut e, &secs(&img)).unwrap();
    assert_eq!((r.cpu1_sections, r.cpu2_sections, r.chunks, r.bytes), (3, 0, 3, 350));
    // CPU1: |1, |3, |7 (trans.c:759); CPU1 done 0xFFFF (:775); CPU2 has nothing and writes 0xFFFFFFFF (:778)
    assert_eq!(e.ladder, [0x1, 0x3, 0x7, 0xFFFF, 0xFFFF_FFFF]);
    // WFPM_GP2 first, then RELEASE_CPU_RESET, before any chunk (trans.c:993, :997)
    let first_prph: Vec<&Op> = e.io.ops.iter().filter(|o| matches!(o, Op::PrphW(..))).take(2).collect();
    assert_eq!(first_prph, [&Op::PrphW(0xA0_30B4, 0x0101_0101), &Op::PrphW(0x300c, 1 << 24)]);
    assert_eq!(e.ints, 2, "iwl_enable_interrupts after each CPU's sections (trans.c:764)");
    assert_eq!(e.waits, 3);
}

#[test]
fn a_regular_style_image_splits_at_the_separator_and_the_second_cpu_shifts_its_ladder_by_sixteen() {
    // CPU1: 2 sections | separator | CPU2: 2 sections | paging separator | paging (not loaded here)
    let img = [sec(0x100, 10), sec(0x200, 20), (SEP, vec![0; 4]), sec(0x300, 30), sec(0x400, 40), (PSEP, vec![0; 4]), sec(0x0, 664), sec(0x100_0000, 8192)];
    let mut e = Env::new();
    let r = load_given_ucode_8000(&mut e, &secs(&img)).unwrap();
    assert_eq!((r.cpu1_sections, r.cpu2_sections, r.bytes), (2, 2, 100));
    // CPU2: sec_num restarts at 1 (trans.c:721) and is shifted by 16 (:728): 0x10000, 0x30000
    assert_eq!(e.ladder, [0x1, 0x3, 0xFFFF, 0x1_FFFF, 0x3_FFFF, 0xFFFF_FFFF]);
    assert_eq!(e.staged.len(), 4, "the separators and the paging sections are not staged");
}

#[test]
fn a_section_is_cut_into_128_kib_chunks_with_the_destination_advancing() {
    let img = [sec(0x0080_0000, 0x2_0000 * 2 + 7)];
    let mut e = Env::new();
    let r = load_given_ucode_8000(&mut e, &secs(&img)).unwrap();
    assert_eq!(r.chunks, 3);
    assert_eq!(e.staged.iter().map(|s| s.0).collect::<Vec<_>>(), [0x2_0000, 0x2_0000, 7]);
    let dsts: Vec<u32> = e.io.ops.iter().filter_map(|o| if let Op::CsrW(0x19c8, v) = o { Some(*v) } else { None }).collect();
    assert_eq!(dsts, [0x0080_0000, 0x0082_0000, 0x0084_0000]); // dst_addr = section->offset + offset (trans.c:685)
    // the byte count is the low 28 bits of TFDIB_CTRL1 (hi address bits above): a 128 KiB chunk needs more than 16 bits
    let ctrl1: Vec<u32> = e.io.ops.iter().filter_map(|o| if let Op::CsrW(0x194c, v) = o { Some(*v) } else { None }).collect();
    assert_eq!(ctrl1, [(3 << 28) | 0x2_0000, (3 << 28) | 0x2_0000, (3 << 28) | 7]);
}

#[test]
fn a_short_section_uses_a_short_chunk() {
    // chunk_sz = min(FH_MEM_TB_MAX_LENGTH, section->len) (trans.c:663)
    let img = [sec(0x10, 5)];
    let mut e = Env::new();
    load_given_ucode_8000(&mut e, &secs(&img)).unwrap();
    assert_eq!(e.staged.iter().map(|s| s.0).collect::<Vec<_>>(), [5]);
}

#[test]
fn chunks_in_the_extended_window_are_bracketed_by_lmpm_chick() {
    // 0x40000..=0x57FFF (trans.c:75-76): the bit is set BEFORE the copy and the chunk, and cleared after the chunk (trans.c:691-701)
    for (dst, bracket) in [(0x3_FFFF, false), (0x4_0000, true), (0x5_7FFF, true), (0x5_8000, false)] {
        let img = [sec(dst, 4)];
        let mut e = Env::new();
        e.io.prph.insert(0xA0_1FF8, 0x80); // an unrelated bit that must survive the read-modify-write
        load_given_ucode_8000(&mut e, &secs(&img)).unwrap();
        let chick: Vec<u32> = e.io.ops.iter().filter_map(|o| if let Op::PrphW(0xA0_1FF8, v) = o { Some(*v) } else { None }).collect();
        assert_eq!(chick, if bracket { vec![0x81, 0x80] } else { vec![] }, "dst {dst:#x}");
        if bracket {
            let ops = &e.io.ops;
            let set = ops.iter().position(|o| *o == Op::PrphW(0xA0_1FF8, 0x81)).unwrap();
            let staged = ops.iter().position(|o| *o == Op::Sleep(0xCAFE)).unwrap();
            let go = ops.iter().position(|o| *o == Op::CsrW(0x1e20, 0x8010_0000)).unwrap();
            let clr = ops.iter().position(|o| *o == Op::PrphW(0xA0_1FF8, 0x80)).unwrap();
            assert!(set < staged && staged < go && go < clr);
        }
    }
}

#[test]
fn a_chunk_that_never_completes_stops_the_upload_names_where_and_still_clears_chick() {
    let img = [sec(0x100, 10), sec(0x4_8000, 4), sec(0x300, 10)];
    let mut e = Env::new();
    e.time_out_at = Some(2);
    let err = load_given_ucode_8000(&mut e, &secs(&img)).unwrap_err();
    assert_eq!(err, LoadError::ChunkTimeout { section: 1, dst: 0x4_8000, offset: 0 }); // trans.c:647-653
    assert_eq!(e.staged.len(), 2, "nothing after the failed chunk");
    assert_eq!(e.ladder, [0x1], "the failed section was never announced");
    assert!(e.io.ops.contains(&Op::PrphW(0xA0_1FF8, 0)), "CHICK is cleared even on failure (trans.c:699-701 precede the check at :703)");
    assert_eq!(e.ints, 0, "interrupts are not enabled for a CPU whose load failed");
}

#[test]
fn a_host_that_cannot_stage_a_chunk_is_a_named_refusal_with_nothing_programmed_for_it() {
    let img = [sec(0x100, 10), sec(0x200, 10)];
    let mut e = Env::new();
    e.refuse_stage_at = Some(1);
    assert_eq!(load_given_ucode_8000(&mut e, &secs(&img)).unwrap_err(), LoadError::Stage(LoadRefusal::NoBounceBuffer));
    assert_eq!(e.waits, 1);
}

#[test]
fn a_bounce_buffer_above_36_bits_is_refused_before_the_registers_are_written() {
    let img = [sec(0x100, 10)];
    let mut e = Env::new();
    e.next_phys = 1 << 36;
    assert_eq!(load_given_ucode_8000(&mut e, &secs(&img)).unwrap_err(), LoadError::DmaAddrTooWide { phys: 1 << 36 }); // iwl-fh.h:597
    assert!(!e.io.ops.iter().any(|o| matches!(o, Op::CsrW(0x1e20, _))));
}

#[test]
fn a_failed_nic_access_grab_is_the_loaders_error() {
    let img = [sec(0x100, 10)];
    let mut e = Env::new();
    e.io.fail_grab = true;
    assert!(matches!(load_given_ucode_8000(&mut e, &secs(&img)).unwrap_err(), LoadError::NicAccess(NicFault::AccessTimeout { .. }))); // trans.c:640-641
}

#[test]
fn the_dma_hi_address_is_four_bits() {
    assert_eq!(dma_hi_addr(0x0000_0003_dead_b000), 3);
    assert_eq!(dma_hi_addr(0x0000_000f_0000_0000), 0xf);
    assert_eq!(dma_hi_addr(0x0000_0010_0000_0000), 0, "bit 36 is masked off: iwl-fh.h:620-623 keeps 4 bits");
}
