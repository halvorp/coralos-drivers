// SPDX-License-Identifier: GPL-2.0-only
//! The durable "begun" record (plan KBL-I915-PORT-PLAN section 5): "a durable 'stage begun' record on the stick before each risky
//! write and a refusal on the next boot if 'begun' without 'done'".
//!
//! The record is a LOG LINE with an explicit step id - the device's task log is what the owner's relay (and later the network log)
//! persists, so the line is printed BEFORE the first risky write of the step and flushed by the log sink as any line is:
//!
//! ```text
//! [I915-STEP begun id=0x3003 name=ELSP-SUBMIT seq=7]
//! [I915-STEP done id=0x3003 name=ELSP-SUBMIT seq=7]          (or)
//! [I915-STEP failed id=0x3003 name=ELSP-SUBMIT seq=7 why=ENGINE-HUNG]
//! ```
//!
//! `failed` ENDS a step (the device stopped with a named failure and said so); only `begun` with no later `done`/`failed` for the
//! same id means the device died inside the step. [`check_previous_boot`] reads the records back and refuses to start a new risky
//! step in that case, which is what stops an automatic retry boot loop from re-running the write that hung the machine.
use super::ExecError;

/// A stable step id. Ranges: 0x2xxx P2 memory, 0x3xxx P3 execution, 0x4xxx display plans.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepId(pub u32);

impl StepId {
    pub const GT_EFFECTS: StepId = StepId(0x3000);
    pub const GGTT_BIND: StepId = StepId(0x2001);
    pub const PPGTT_BIND: StepId = StepId(0x2002);
    pub const MEMORY_QUALIFY: StepId = StepId(0x2003);
    pub const ENGINE_ENABLE: StepId = StepId(0x3001);
    pub const CSB_RESET: StepId = StepId(0x3002);
    pub const ELSP_SUBMIT: StepId = StepId(0x3003);
    pub const ENGINE_RESET: StepId = StepId(0x3004);
    pub const FULL_RESET: StepId = StepId(0x3005);
    pub const DISPLAY_CLOCK: StepId = StepId(0x4001);
    pub const DISPLAY_LINK: StepId = StepId(0x4002);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Begun,
    Done,
    Failed,
}

impl Phase {
    pub const fn word(self) -> &'static str {
        match self {
            Phase::Begun => "begun",
            Phase::Done => "done",
            Phase::Failed => "failed",
        }
    }
}

/// One parsed record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepRecord {
    pub id: StepId,
    pub phase: Phase,
}

const PREFIX: &str = "[I915-STEP ";

/// Parse one log line. `None` for any line that is not a step record.
pub fn parse_record(line: &str) -> Option<StepRecord> {
    let rest = line.trim_start().strip_prefix(PREFIX)?;
    let (word, rest) = rest.split_once(' ')?;
    let phase = match word {
        "begun" => Phase::Begun,
        "done" => Phase::Done,
        "failed" => Phase::Failed,
        _ => return None,
    };
    let hex = rest.strip_prefix("id=0x")?;
    let end = hex.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(hex.len());
    let id = u32::from_str_radix(&hex[..end], 16).ok()?;
    Some(StepRecord { id: StepId(id), phase })
}

/// The next-boot rule: any `begun` that no later `done`/`failed` of the same id ended refuses the boot's risky steps, naming the
/// FIRST such id.
pub fn check_previous_boot<'a>(lines: impl Iterator<Item = &'a str>) -> Result<(), ExecError> {
    // Fixed on purpose: the table is keyed by STEP ID, an enumerated set this file defines (about a dozen), not a runtime-growing
    // population; a log naming more distinct ids than this was not written by this device and is refused by name.
    // At most one step is open at a time (the executor refuses nesting), but a log may hold many boots: track the latest phase per id
    // in a small fixed table.
    let mut seen: [(u32, Phase); 32] = [(0, Phase::Done); 32];
    let mut n = 0usize;
    for rec in lines.filter_map(parse_record) {
        match seen[..n].iter().position(|(id, _)| *id == rec.id.0) {
            Some(i) => seen[i].1 = rec.phase,
            None if n < seen.len() => {
                seen[n] = (rec.id.0, rec.phase);
                n += 1;
            }
            // A log with more than 32 distinct step ids is not one this device wrote: refuse rather than guess.
            None => return Err(ExecError::Bound { what: "distinct step ids in the previous log", cap: seen.len() as u32 }),
        }
    }
    match seen[..n].iter().find(|(_, p)| *p == Phase::Begun) {
        Some((id, _)) => Err(ExecError::PreviousStepNotDone { id: *id }),
        None => Ok(()),
    }
}
