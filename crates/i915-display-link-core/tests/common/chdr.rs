// SPDX-License-Identifier: GPL-2.0-only
//! An independent oracle for register / DPCD constants: evaluate the object-like `#define NAME EXPR` straight out of the pinned
//! Linux header at test time (`REG_BIT`, `REG_GENMASK`, `REG_FIELD_PREP`, `_MMIO`, shifts, `|`, `+`, `-`, `*`, parentheses and
//! references to other macros of the same header). A constant typed into the Rust crate is therefore compared with what the
//! header says, not with another copy of what the porter read (PORT_CONVENTIONS.md 3b "coverage is not correctness").
//! The test SKIPS (loudly) when the Linux tree is not on this host.
#![allow(dead_code)]
use std::collections::HashMap;

pub fn linux_ref() -> String {
    std::env::var("I915_LINUX_REF").unwrap_or_else(|_| "/home/peter/Projects/references/coralos/linux-ref".into())
}

pub struct Header {
    defs: HashMap<String, String>,
}

impl Header {
    /// `rel` is relative to the linux-ref root, e.g. `drivers/gpu/drm/i915/display/intel_pps_regs.h`.
    pub fn load(rel: &str) -> Option<Header> {
        let text = std::fs::read_to_string(format!("{}/{}", linux_ref(), rel)).ok()?;
        // join continuation lines, drop /* */ comments
        let mut joined = String::new();
        for l in text.lines() {
            if let Some(s) = l.strip_suffix('\\') {
                joined.push_str(s);
                joined.push(' ');
            } else {
                joined.push_str(l);
                joined.push('\n');
            }
        }
        let mut stripped = String::new();
        let mut rest = joined.as_str();
        while let Some(i) = rest.find("/*") {
            stripped.push_str(&rest[..i]);
            match rest[i..].find("*/") {
                Some(j) => rest = &rest[i + j + 2..],
                None => {
                    rest = "";
                }
            }
        }
        stripped.push_str(rest);
        let mut defs = HashMap::new();
        for l in stripped.lines() {
            let t = l.trim_start();
            let Some(t) = t.strip_prefix('#') else { continue };
            let Some(t) = t.trim_start().strip_prefix("define") else { continue };
            let t = t.trim_start();
            let end = t.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(t.len());
            let name = &t[..end];
            let after = &t[end..];
            if after.starts_with('(') {
                continue; // function-like macro
            }
            // strip // comments too
            let val = after.split("//").next().unwrap().trim();
            defs.insert(name.to_string(), val.to_string());
        }
        Some(Header { defs })
    }

    pub fn eval(&self, name: &str) -> Option<u64> {
        let v = self.defs.get(name)?;
        let mut p = Parser { h: self, toks: tokenize(v), i: 0 };
        let r = p.or();
        if p.i == p.toks.len() {
            r
        } else {
            None
        }
    }
}

fn tokenize(s: &str) -> Vec<String> {
    let b: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_alphanumeric() || c == '_' {
            let st = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            out.push(b[st..i].iter().collect());
        } else if c == '<' && i + 1 < b.len() && b[i + 1] == '<' {
            out.push("<<".into());
            i += 2;
        } else {
            out.push(c.to_string());
            i += 1;
        }
    }
    out
}

struct Parser<'a> {
    h: &'a Header,
    toks: Vec<String>,
    i: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.i).map(|s| s.as_str())
    }
    fn or(&mut self) -> Option<u64> {
        let mut v = self.shift()?;
        while self.peek() == Some("|") {
            self.i += 1;
            v |= self.shift()?;
        }
        Some(v)
    }
    fn shift(&mut self) -> Option<u64> {
        let mut v = self.add()?;
        while self.peek() == Some("<<") {
            self.i += 1;
            let r = self.add()?;
            v = v.checked_shl(r as u32)?;
        }
        Some(v)
    }
    fn add(&mut self) -> Option<u64> {
        let mut v = self.mul()?;
        loop {
            match self.peek() {
                Some("+") => {
                    self.i += 1;
                    v = v.wrapping_add(self.mul()?);
                }
                Some("-") => {
                    self.i += 1;
                    v = v.wrapping_sub(self.mul()?);
                }
                _ => return Some(v),
            }
        }
    }
    fn mul(&mut self) -> Option<u64> {
        let mut v = self.atom()?;
        while self.peek() == Some("*") {
            self.i += 1;
            v = v.wrapping_mul(self.atom()?);
        }
        Some(v)
    }
    fn args(&mut self) -> Option<Vec<u64>> {
        // after '('
        let mut a = vec![];
        loop {
            a.push(self.or()?);
            match self.peek() {
                Some(",") => self.i += 1,
                Some(")") => {
                    self.i += 1;
                    return Some(a);
                }
                _ => return None,
            }
        }
    }
    fn atom(&mut self) -> Option<u64> {
        let t = self.peek()?.to_string();
        self.i += 1;
        if t == "(" {
            let v = self.or()?;
            if self.peek() == Some(")") {
                self.i += 1;
                return Some(v);
            }
            return None;
        }
        if t.starts_with(|c: char| c.is_ascii_digit()) {
            let t = t.trim_end_matches(|c| c == 'u' || c == 'U' || c == 'l' || c == 'L');
            return if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) { u64::from_str_radix(h, 16).ok() } else { t.parse().ok() };
        }
        if self.peek() == Some("(") {
            self.i += 1;
            let a = self.args()?;
            return match t.as_str() {
                "REG_BIT" | "BIT" => Some(1u64 << a[0]),
                "REG_GENMASK" => Some(((1u64 << (a[0] + 1)) - 1) & !((1u64 << a[1]) - 1)),
                "REG_FIELD_PREP" => Some((a[1] << a[0].trailing_zeros()) & a[0]),
                "_MMIO" => Some(a[0]),
                _ => None,
            };
        }
        // an object-like macro of the same header
        let v = self.h.defs.get(&t)?.clone();
        let mut p = Parser { h: self.h, toks: tokenize(&v), i: 0 };
        let r = p.or();
        if p.i == p.toks.len() {
            r
        } else {
            None
        }
    }
}
