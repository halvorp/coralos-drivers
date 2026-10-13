// SPDX-License-Identifier: GPL-2.0-only
//! The queue and the ring accounting: whole frames, wrap, short writes, the hardware's position folded in, the underrun rule.
use avs_device::stream::*;

#[test]
fn the_queue_takes_whole_frames_wraps_and_never_exceeds_its_capacity() {
    let mut f = Fifo::new(16).unwrap();
    assert_eq!((f.capacity(), f.queued(), f.free()), (16, 0, 16));
    assert_eq!(f.push(&[1; 7], 4), 4, "a partial frame is not taken");
    assert_eq!(f.push(&[2; 40], 4), 12, "what fits, in whole frames");
    assert_eq!((f.queued(), f.free()), (16, 0));
    assert_eq!(f.push(&[3; 4], 4), 0);
    let mut out = [0u8; 6];
    assert_eq!(f.pop_into(&mut out, 4), 4, "pops whole frames only");
    assert_eq!(out[..4], [1, 1, 1, 1]);
    // wrap: push after pop
    assert_eq!(f.push(&[9; 4], 4), 4);
    let mut all = [0u8; 16];
    assert_eq!(f.pop_into(&mut all, 4), 16);
    assert_eq!(all, [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 9, 9, 9, 9]);
    f.push(&[5; 8], 4);
    f.clear();
    assert_eq!((f.queued(), f.free()), (0, 16));
}

#[test]
fn a_heap_that_cannot_give_the_queue_is_none_not_an_abort() {
    assert!(Fifo::new(usize::MAX / 2).is_none());
}

#[test]
fn the_hardware_position_is_wrap_corrected_and_a_dead_register_is_not_a_position() {
    let mut a = RingAcct::new(16384, 4096, 4);
    a.commit(Fill { pcm: 16384, silence: 0 }, false);
    assert!(a.advance_hw(4096));
    assert!(a.advance_hw(12288));
    assert_eq!(a.hw_total, 12288);
    assert!(a.advance_hw(2048), "wrapped past the end of the ring");
    assert_eq!(a.hw_total, 16384 + 2048);
    assert_eq!(a.played_frames(), (16384 + 2048) / 4);
    let before = a;
    assert!(!a.advance_hw(0xffff_ffff), "all-ones is a dead register");
    assert!(!a.advance_hw(16384), "a position AT the ring's size is not inside it");
    assert_eq!(a, before);
    assert!(a.advance_hw(2048), "no movement is zero bytes");
    assert_eq!(a.hw_total, before.hw_total);
}

#[test]
fn the_refill_moves_pcm_first_and_silence_only_to_keep_one_period_ahead() {
    let mut a = RingAcct::new(16384, 4096, 4);
    // priming: the whole ring, all silence, no underrun
    let f = a.plan_fill(0, 16384);
    assert_eq!(f, Fill { pcm: 0, silence: 16384 });
    a.commit(f, false);
    assert_eq!((a.fed, a.underruns, a.write_pos()), (16384, 0, 0));
    // the hardware read two periods
    a.advance_hw(8192);
    // 6000 bytes queued: all of it goes in (room for 8192), and with 8192 + 6000 in flight > one period there is no silence
    let f = a.plan_fill(6000, 4096);
    assert_eq!(f, Fill { pcm: 6000, silence: 0 });
    a.commit(f, true);
    assert_eq!((a.pcm_fed, a.underruns), (6000, 0));
    // the hardware reads on until 2384 bytes are left; an empty queue now means silence up to one period, and it counts
    let mut b = RingAcct::new(16384, 4096, 4);
    b.commit(Fill { pcm: 16384, silence: 0 }, false);
    assert!(b.advance_hw(14000));
    let f = b.plan_fill(0, 4096);
    assert_eq!(f, Fill { pcm: 0, silence: 1712 });
    b.commit(f, true);
    assert_eq!((b.underruns, b.pcm_fed), (1, 16384));
    // and with real data queued there is no silence and no underrun
    let mut c = RingAcct::new(16384, 4096, 4);
    c.commit(Fill { pcm: 16384, silence: 0 }, false);
    c.advance_hw(14000);
    let f = c.plan_fill(4096, 4096);
    assert_eq!(f, Fill { pcm: 4096, silence: 0 });
}

#[test]
fn a_fill_never_overruns_the_ring_and_is_always_whole_frames() {
    for queued in [0usize, 1, 3, 4, 5, 4096, 16384, 100_000] {
        for hw in [0u32, 4, 5, 4096, 8191, 16380] {
            let mut a = RingAcct::new(16384, 4096, 4);
            a.commit(Fill { pcm: 8192, silence: 0 }, false);
            a.advance_hw(hw.min(8192) / 4 * 4);
            let f = a.plan_fill(queued, 4096);
            assert_eq!(f.pcm % 4, 0);
            assert_eq!(f.silence % 4, 0);
            assert!(f.pcm + f.silence <= 16384 - a.in_flight() as usize, "queued {queued} hw {hw}: {f:?} overruns the ring");
            assert!(f.pcm <= queued);
        }
    }
}

#[test]
fn a_hardware_that_ran_past_everything_written_is_an_underrun_and_the_write_position_resyncs_on_a_frame() {
    let mut a = RingAcct::new(16384, 4096, 4);
    a.commit(Fill { pcm: 4096, silence: 0 }, false);
    // the hardware read 4096 + 1700 + 2 bytes: LPIB need not be on a frame boundary
    assert!(a.advance_hw(4096));
    assert!(a.advance_hw(4096 + 1702));
    assert!(a.underruns >= 1);
    assert_eq!(a.fed % 4, 0, "the resynced write position is a whole frame ({})", a.fed);
    assert!(a.fed >= a.hw_total && a.fed - a.hw_total < 4);
    assert_eq!(a.write_pos() % 4, 0);
}
