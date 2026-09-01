// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for STATUS card states.
//! Ported from `include/linux/mmc/mmc.h` and `drivers/mmc/core/mmc_ops.c`.
//! Copyright 2006-2007 Pierre Ossman and the Linux MMC authors.
use mmc_core_cmd::status::*;

#[test]
fn every_linux_card_state_is_pinned_by_count_and_name() {
    let got: Vec<_> = CARD_STATES.iter().map(|x|(x.name,x.value)).collect();
    assert_eq!(got, [("IDLE",0),("READY",1),("IDENT",2),("STBY",3),("TRAN",4),("DATA",5),("RCV",6),("PRG",7),("DIS",8)]); // mmc.h:160-168
}
#[test]
fn current_state_extracts_bits_nine_through_twelve() {
    assert_eq!(CURRENT_STATE_MASK,0x1e00); assert_eq!(CURRENT_STATE_SHIFT,9); // mmc.h:154
    assert_eq!(current_state(0x0800),Ok(CardState::Transfer));
    assert_eq!(current_state(0x0e00),Ok(CardState::Programming));
    assert_eq!(current_state(0x1e00),Err(StatusError::ReservedCardState{value:15,maximum_defined:8}));
}
#[test]
fn readiness_requires_both_ready_bit_and_transfer_state() {
    assert_eq!(READY_FOR_DATA,0x100); // mmc.h:155
    assert!(ready_for_data(0x0900)); assert!(!ready_for_data(0x0800)); assert!(!ready_for_data(0x0f00)); // mmc.h:170-177
}
