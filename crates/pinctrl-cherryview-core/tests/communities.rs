// SPDX-License-Identifier: GPL-2.0-only
//! Frozen community vectors from Linux `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//! Copyright (C) 2014-2020 Intel Corporation; Mika Westerberg, Ning Li, Alan Cox.

use pinctrl_cherryview_core::communities::{community_by_uid, gpio_to_pin, COMMUNITIES};

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
    let bank_names: Vec<_> = COMMUNITIES
        .iter()
        .flat_map(|c| c.banks.iter().map(|b| b.name))
        .collect();
    assert_eq!(bank_names.len(), 20); // 7 + 5 + 2 + 6, pinctrl-cherryview.c:254-261,350-355,402-404,522-528
    assert_eq!(
        bank_names,
        vec![
            "southwest_gpp0",
            "southwest_gpp1",
            "southwest_gpp2",
            "southwest_gpp3",
            "southwest_gpp4",
            "southwest_gpp5",
            "southwest_gpp6",
            "north_gpp0",
            "north_gpp1",
            "north_gpp2",
            "north_gpp3",
            "north_gpp4",
            "east_gpp0",
            "east_gpp1",
            "southeast_gpp0",
            "southeast_gpp1",
            "southeast_gpp2",
            "southeast_gpp3",
            "southeast_gpp4",
            "southeast_gpp5",
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
