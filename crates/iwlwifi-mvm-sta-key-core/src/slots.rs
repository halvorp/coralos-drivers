// SPDX-License-Identifier: GPL-2.0-only
//! The firmware key-slot table: `mvm->fw_key_table` (a bitmap) and `mvm->fw_key_deleted`
//! (how long ago each slot was freed). `iwl_mvm_set_fw_key_idx` (sta.c:3455) picks the free slot
//! that was freed longest ago; `iwl_mvm_remove_sta_key` (sta.c:3943-3982) ages every counter and
//! zeroes the one just freed. The sixteen slots are the firmware's, not a policy cap.

use crate::ids::{STA_KEY_IDX_INVALID, STA_KEY_MAX_NUM};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRefusal {
    /// All sixteen slots are in use (`-ENOSPC`, sta.c:3911).
    Full,
    /// Freeing a slot that is not in use (`-ENOENT`, sta.c:3969).
    NotInUse(u8),
}

pub struct KeySlots {
    used: [bool; STA_KEY_MAX_NUM],
    deleted: [u8; STA_KEY_MAX_NUM],
}

impl Default for KeySlots {
    fn default() -> Self {
        KeySlots::new()
    }
}

impl KeySlots {
    pub const fn new() -> KeySlots {
        KeySlots { used: [false; STA_KEY_MAX_NUM], deleted: [0; STA_KEY_MAX_NUM] }
    }

    /// Reserve the free slot with the highest deleted counter (the lowest index on a tie, as the
    /// strict `>` of sta.c:3471 keeps the first maximum).
    pub fn alloc(&mut self) -> Result<u8, SlotRefusal> {
        let mut best: Option<usize> = None;
        for i in 0..STA_KEY_MAX_NUM {
            if self.used[i] {
                continue;
            }
            match best {
                Some(b) if self.deleted[i] <= self.deleted[b] => {}
                _ => best = Some(i),
            }
        }
        match best {
            Some(i) => {
                self.used[i] = true;
                debug_assert_ne!(i as u8, STA_KEY_IDX_INVALID);
                Ok(i as u8)
            }
            None => Err(SlotRefusal::Full),
        }
    }

    /// Give a slot back: ages every counter (saturating at 255) and zeroes this slot's.
    pub fn free(&mut self, slot: u8) -> Result<(), SlotRefusal> {
        let i = slot as usize;
        if i >= STA_KEY_MAX_NUM || !self.used[i] {
            return Err(SlotRefusal::NotInUse(slot));
        }
        self.used[i] = false;
        for d in self.deleted.iter_mut() {
            if *d < u8::MAX {
                *d += 1;
            }
        }
        self.deleted[i] = 0;
        Ok(())
    }

    pub fn in_use(&self, slot: u8) -> bool {
        self.used.get(slot as usize).copied().unwrap_or(false)
    }
}
