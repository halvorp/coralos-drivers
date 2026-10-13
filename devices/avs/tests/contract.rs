// SPDX-License-Identifier: GPL-2.0-only
//! The isolation contract: avs.device links only coralos-rt, coralos-hal, the permissive avs-wire and the public port crates - never a closed
//! CoralOS crate - and its sources name none. The checker is shown to be able to fail, not only to pass.
use std::fs;
use std::path::Path;

const ALLOWED_EXACT: [&str; 4] = ["coralos-rt", "coralos-hal", "avs-wire", "hda-ext-core"];
const ALLOWED_PREFIX: [&str; 4] = ["avs-", "da7219-", "i2c-", "hda-"];

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

fn forbidden(toml: &str) -> Vec<String> {
    dependency_names(toml).into_iter().filter(|n| !ALLOWED_EXACT.contains(&n.as_str()) && !ALLOWED_PREFIX.iter().any(|p| n.starts_with(p))).collect()
}

const CLOSED: [&str; 8] = ["coral_i915_gem", "coral_drm", "coral_hal", "coral_libuser", "coral_sys", "coral_firmament", "coral_syscall", "coral_audio"];

fn sources_naming_closed(dir: &Path, hits: &mut Vec<String>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            sources_naming_closed(&p, hits);
        } else if p.extension().is_some_and(|x| x == "rs") {
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
    assert!(dependency_names(&toml).len() >= 10);
}

#[test]
fn the_sources_name_no_closed_crate() {
    let mut hits = vec![];
    sources_naming_closed(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut hits);
    assert_eq!(hits, Vec::<String>::new());
}

#[test]
fn the_checker_can_fail() {
    let bad = "[dependencies]\ncoralos-rt = { path = \"x\" }\ncoral-libuser-x86 = { path = \"y\" }\nserde = \"1\"\n[dev-dependencies]\navs-wire = \"1\"\n";
    assert_eq!(forbidden(bad), vec!["coral-libuser-x86".to_string(), "serde".to_string()]);
    let dir = std::env::temp_dir().join("avs-contract-selftest");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("x.rs"), "use coral_sys::thing;\n// coral_drm in a comment is fine\n").unwrap();
    let mut hits = vec![];
    sources_naming_closed(&dir, &mut hits);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].contains("coral_sys"));
}

#[test]
fn the_wire_crate_is_permissive_and_the_device_is_gpl() {
    let wire = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wire/avs-wire/Cargo.toml")).unwrap();
    assert!(wire.contains("license = \"MIT OR Apache-2.0\""));
    assert!(dependency_names(&wire).is_empty(), "the permissive wire crate depends on nothing");
    let dev = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(dev.contains("license = \"GPL-2.0-only\""));
}
