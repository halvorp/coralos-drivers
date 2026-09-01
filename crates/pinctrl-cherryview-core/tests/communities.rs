// SPDX-License-Identifier: GPL-2.0-only
//! Frozen community vectors from Linux `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//! Copyright (C) 2014-2020 Intel Corporation; Mika Westerberg, Ning Li, Alan Cox.

use pinctrl_cherryview_core::communities::{
    community_by_uid, gpio_to_pin, EAST_BANKS, NORTH_BANKS, SOUTHEAST_BANKS, SOUTHWEST_BANKS,
    COMMUNITIES,
};

#[test]
fn all_communities_and_banks_are_present_and_named() {
    // chv_soc_data[] has four non-NULL records, pinctrl-cherryview.c:547-552.
    assert_eq!(COMMUNITIES.len(), 4);
    let communities: Vec<_> = COMMUNITIES
        .iter()
        .map(|c| (c.name, c.uid, c.acpi_space_id, c.interrupt_lines))
        .collect();
    assert_eq!(
        communities,
        vec![
            ("southwest", "1", 0x91, 8),  // pinctrl-cherryview.c:268-280
            ("north", "2", 0x92, 8),      // pinctrl-cherryview.c:362-370
            ("east", "3", 0x93, 16),      // pinctrl-cherryview.c:407-415
            ("southeast", "4", 0x94, 16), // pinctrl-cherryview.c:531-543
        ]
    );
}

#[test]
fn southwest_bank_names_count_and_ranges_match_linux() {
    // southwest_gpps[] has seven entries, pinctrl-cherryview.c:254-261.
    assert_eq!(SOUTHWEST_BANKS.len(), 7);
    let banks: Vec<_> = SOUTHWEST_BANKS
        .iter()
        .map(|bank| (bank.name, bank.first_pin, bank.last_pin, bank.gpio_base))
        .collect();
    assert_eq!(
        banks,
        vec![
            ("southwest_gpp0", 0, 7, 0),    // pinctrl-cherryview.c:255
            ("southwest_gpp1", 15, 22, 15), // pinctrl-cherryview.c:256
            ("southwest_gpp2", 30, 37, 30), // pinctrl-cherryview.c:257
            ("southwest_gpp3", 45, 52, 45), // pinctrl-cherryview.c:258
            ("southwest_gpp4", 60, 67, 60), // pinctrl-cherryview.c:259
            ("southwest_gpp5", 75, 82, 75), // pinctrl-cherryview.c:260
            ("southwest_gpp6", 90, 97, 90), // pinctrl-cherryview.c:261
        ]
    );
}

#[test]
fn north_bank_names_count_and_ranges_match_linux() {
    // north_gpps[] has five entries, pinctrl-cherryview.c:350-355.
    assert_eq!(NORTH_BANKS.len(), 5);
    let banks: Vec<_> = NORTH_BANKS
        .iter()
        .map(|bank| (bank.name, bank.first_pin, bank.last_pin, bank.gpio_base))
        .collect();
    assert_eq!(
        banks,
        vec![
            ("north_gpp0", 0, 8, 0),    // pinctrl-cherryview.c:351
            ("north_gpp1", 15, 27, 15), // pinctrl-cherryview.c:352
            ("north_gpp2", 30, 41, 30), // pinctrl-cherryview.c:353
            ("north_gpp3", 45, 56, 45), // pinctrl-cherryview.c:354
            ("north_gpp4", 60, 72, 60), // pinctrl-cherryview.c:355
        ]
    );
}

#[test]
fn east_bank_names_count_and_ranges_match_linux() {
    // east_gpps[] has two entries, pinctrl-cherryview.c:402-404.
    assert_eq!(EAST_BANKS.len(), 2);
    let banks: Vec<_> = EAST_BANKS
        .iter()
        .map(|bank| (bank.name, bank.first_pin, bank.last_pin, bank.gpio_base))
        .collect();
    assert_eq!(
        banks,
        vec![
            ("east_gpp0", 0, 11, 0),    // pinctrl-cherryview.c:403
            ("east_gpp1", 15, 26, 15), // pinctrl-cherryview.c:404
        ]
    );
}

#[test]
fn southeast_bank_names_count_and_ranges_match_linux() {
    // southeast_gpps[] has six entries, pinctrl-cherryview.c:522-528.
    assert_eq!(SOUTHEAST_BANKS.len(), 6);
    let banks: Vec<_> = SOUTHEAST_BANKS
        .iter()
        .map(|bank| (bank.name, bank.first_pin, bank.last_pin, bank.gpio_base))
        .collect();
    assert_eq!(
        banks,
        vec![
            ("southeast_gpp0", 0, 7, 0),    // pinctrl-cherryview.c:523
            ("southeast_gpp1", 15, 26, 15), // pinctrl-cherryview.c:524
            ("southeast_gpp2", 30, 35, 30), // pinctrl-cherryview.c:525
            ("southeast_gpp3", 45, 52, 45), // pinctrl-cherryview.c:526
            ("southeast_gpp4", 60, 69, 60), // pinctrl-cherryview.c:527
            ("southeast_gpp5", 75, 85, 75), // pinctrl-cherryview.c:528
        ]
    );
}

#[test]
fn uid_lookup_and_gpio_bank_translation_have_vectors() {
    let southwest = community_by_uid("1").unwrap();
    assert_eq!(southwest.name, "southwest"); // pinctrl-cherryview.c:272-274
    assert!(community_by_uid("5").is_none());
    assert_eq!(gpio_to_pin(southwest, 0), Some(0)); // pinctrl-cherryview.c:255
    assert_eq!(gpio_to_pin(southwest, 15), Some(15)); // pinctrl-cherryview.c:256
    assert_eq!(gpio_to_pin(southwest, 97), Some(97)); // pinctrl-cherryview.c:261
    assert_eq!(gpio_to_pin(southwest, 8), None); // gap between :255 and :256
}
