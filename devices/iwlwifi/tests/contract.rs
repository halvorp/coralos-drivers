// SPDX-License-Identifier: GPL-2.0-only
//! The isolation contract: iwlwifi.device links only coralos-rt, coralos-hal and the public port crates - never a closed CoralOS crate -
//! and its sources name none. In particular it names no `coral_wifi_abi`: the wire to wifi-manager is the CLOSED wrapper's business
//! (this crate speaks neutral `Request` / `Event` values). The checker is shown to be able to fail, not only to pass.
use std::fs;
use std::path::Path;

const ALLOWED_EXACT: [&str; 2] = ["coralos-rt", "coralos-hal"];
const ALLOWED_PREFIX: [&str; 1] = ["iwlwifi-"];

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

const CLOSED: [&str; 11] = [
    "coral_wifi", "coral_net", "coral_libuser", "coral_sys", "coral_firmament", "coral_syscall", "coral_driver_metadata", "coral_pci", "coral_fmt", "coral_connect",
    "libcoral",
];

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
    let bad = "[dependencies]\ncoralos-rt = { path = \"x\" }\ncoral-wifi-abi = { path = \"y\" }\nserde = \"1\"\n[dev-dependencies]\niwlwifi-pcie-nic-core = \"1\"\n";
    assert_eq!(forbidden(bad), vec!["coral-wifi-abi".to_string(), "serde".to_string()]);
    let dir = std::env::temp_dir().join("iwlwifi-contract-selftest");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("x.rs"), "use coral_wifi_abi::driver::DriverEvent;\n// coral_sys in a comment is fine\n").unwrap();
    let mut hits = vec![];
    sources_naming_closed(&dir, &mut hits);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].contains("coral_wifi"));
}

#[test]
fn the_crate_is_licensed_gpl_and_every_source_says_so() {
    let toml = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(toml.contains("license = \"GPL-2.0-only\""));
    fn walk(d: &Path, bad: &mut Vec<String>) {
        for e in fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, bad);
            } else if p.extension().is_some_and(|x| x == "rs") && !fs::read_to_string(&p).unwrap().starts_with("// SPDX-License-Identifier: GPL-2.0-only") {
                bad.push(p.display().to_string());
            }
        }
    }
    let mut bad = vec![];
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut bad);
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"), &mut bad);
    assert_eq!(bad, Vec::<String>::new());
}
