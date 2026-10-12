// SPDX-License-Identifier: GPL-2.0-only
use i915_kbl_display_core::skl_pfit::*;

#[test]
fn native_mode_needs_no_panel_fit_and_anything_else_is_a_named_refusal() {
    assert_eq!(check_panel_fit(1920, 1080, 1920, 1080), Ok(PanelFit::NotNeeded));
    for (w, h) in [(1280, 720), (1920, 1079), (1919, 1080), (3840, 2160)] {
        let e = check_panel_fit(w, h, 1920, 1080).unwrap_err();
        assert_eq!(e.name(), "PFIT-SCALER-NOT-PORTED");
        assert_eq!(e, PfitError::NotPorted { src_w: w, src_h: h, native_w: 1920, native_h: 1080 });
    }
}
