// SPDX-License-Identifier: GPL-2.0-only
//! The isolation contract (docs/I915-CONTRACTS.md): `devices/i915` links ONLY coralos-rt, coralos-hal, the i915 port crates and
//! the permissive i915-wire - never a closed CoralOS crate - and its sources name none. A closed dependency injected into
//! Cargo.toml (or an import into the sources) must fail HERE; the tests below prove the checker can fail, not only pass.
use std::fs;
use std::path::Path;

/// The dependency names the device may have.
const ALLOWED_EXACT: [&str; 4] = ["coralos-rt", "coralos-hal", "i915-wire", "i915-kbl-display-core"];
const ALLOWED_PREFIX: &str = "i915-";

/// The names of the `[dependencies]` entries of a Cargo.toml text.
fn dependency_names(toml: &str) -> Vec<String> {
    let mut in_deps = false;
    let mut out = vec![];
    for line in toml.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_deps = l == "[dependencies]" || l == "[build-dependencies]" || l == "[dev-dependencies]";
            continue;
        }
        if in_deps && !l.is_empty() && !l.starts_with('#') {
            if let Some((name, _)) = l.split_once('=') {
                out.push(name.trim().to_string());
            }
        }
    }
    out
}

/// The dependencies of `toml` that the contract forbids.
fn forbidden(toml: &str) -> Vec<String> {
    dependency_names(toml).into_iter().filter(|n| !ALLOWED_EXACT.contains(&n.as_str()) && !n.starts_with(ALLOWED_PREFIX)).collect()
}

/// Identifiers of the closed tree that no source file may name.
const CLOSED: [&str; 8] = ["coral_i915_gem", "coral_drm", "coral_hal", "coral_libuser", "coral_sys", "coral_firmament", "coral_syscall", "coral-i915-gem"];

fn sources_naming_closed(dir: &Path, hits: &mut Vec<String>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            sources_naming_closed(&p, hits);
        } else if p.extension().map_or(false, |x| x == "rs") {
            let text = fs::read_to_string(&p).unwrap();
            for (n, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                for c in CLOSED {
                    if code.contains(c) {
                        hits.push(format!("{}:{}: {c}", p.display(), n + 1));
                    }
                }
            }
        }
    }
}

#[test]
fn the_real_manifest_has_only_contract_dependencies() {
    let toml = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert_eq!(forbidden(&toml), Vec::<String>::new());
    let names = dependency_names(&toml);
    for must in ["coralos-rt", "coralos-hal", "i915-wire", "i915-common-core", "i915-gt-regs-core", "i915-devinfo-core", "i915-uncore-core", "i915-kbl-display-core"] {
        assert!(names.iter().any(|n| n == must), "{must} missing from {names:?}");
    }
}

#[test]
fn an_injected_closed_dependency_is_caught() {
    let mut toml = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    toml = toml.replace("[dependencies]\n", "[dependencies]\ncoral-i915-gem = { path = \"../../../coralos/userland/coral-i915-gem\" }\n");
    assert_eq!(forbidden(&toml), vec!["coral-i915-gem".to_string()]);
    let toml2 = "[dependencies]\ncoral-drm-driver = { path = \"x\" }\ncoralos-rt = { path = \"y\" }\ncoral-hal = { path = \"z\" }\nserde = \"1\"\n";
    assert_eq!(forbidden(toml2), vec!["coral-drm-driver".to_string(), "coral-hal".to_string(), "serde".to_string()]);
    // the proof the allow-list is not a prefix trick: a look-alike is not allowed
    assert_eq!(forbidden("[dependencies]\ncoralos-rtx = \"1\"\ni915 = \"1\"\n"), vec!["coralos-rtx".to_string(), "i915".to_string()]);
}

#[test]
fn no_source_names_a_closed_crate() {
    let mut hits = vec![];
    sources_naming_closed(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut hits);
    assert!(hits.is_empty(), "{hits:#?}");
}

#[test]
fn an_import_of_a_closed_crate_in_a_source_is_caught() {
    let dir = std::env::temp_dir().join(format!("i915-contract-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("bad.rs"), "use coral_i915_gem::Gtt;\n").unwrap();
    fs::write(dir.join("fine.rs"), "// mentions coral_drm in a comment only\nfn main() {}\n").unwrap();
    let mut hits = vec![];
    sources_naming_closed(&dir, &mut hits);
    fs::remove_dir_all(&dir).ok();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].contains("bad.rs:1: coral_i915_gem"));
}

#[test]
fn the_device_has_no_dma_or_irq_dependency_yet() {
    // increment 1 is "no DMA, no IRQ": the device sources use neither grant.
    let mut hits = vec![];
    fn walk(dir: &Path, hits: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, hits);
            } else if p.extension().map_or(false, |x| x == "rs") {
                let t = fs::read_to_string(&p).unwrap();
                for (n, l) in t.lines().enumerate() {
                    let code = l.split("//").next().unwrap();
                    if code.contains("DmaRegion") || code.contains("dma_alloc") || code.contains("IrqWait") || code.contains("irq_line") {
                        hits.push(format!("{}:{}", p.display(), n + 1));
                    }
                }
            }
        }
    }
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut hits);
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn the_devinfo_and_display_tables_agree_on_the_kaby_lake_ids() {
    use i915_devinfo_core::{Platform, ID_TABLE};
    use i915_kbl_display_core::platform::KBL_IDS;
    let mut a: Vec<u16> = ID_TABLE.iter().filter(|r| r.info.platform == Platform::Kabylake).map(|r| r.devid).collect();
    let mut b: Vec<u16> = KBL_IDS.iter().map(|r| r.device_id).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b, "two Kaby Lake id tables drifted apart");
    assert_eq!(a.len(), 22);
    // and they agree on the GT of each id
    for k in KBL_IDS.iter() {
        let d = i915_devinfo_core::lookup(k.device_id).unwrap();
        assert_eq!(d.gt(), k.gt, "{:#x}", k.device_id);
    }
}
