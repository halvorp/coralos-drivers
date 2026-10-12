// SPDX-License-Identifier: GPL-2.0-only
//! The library table and the file names of `loader.c`: which slot the next library takes, which libraries are skipped
//! because the DSP already holds them (`avs_dsp_load_libraries`, loader.c:489-552), and the path of the base firmware,
//! of a library and of a loadable module (`avs_dsp_load_basefw` :554-572, `avs_cldma_load_module` :215-230).
use crate::error::FwError;
use crate::regs::AVS_LIB_NAME_SIZE;

/// FIXED BY AN ABI WIDTH (DECISIONS.md D10): `lib_id` is a 4-bit field of LOAD_LIBRARY (messages.h `load_lib.lib_id:4`), so at most 16 slots can ever be addressed.
pub const MAX_LIB_SLOTS: usize = 16; // messages.h:82

/// `AVS_ROOT_DIR` (loader.c:36).
pub const AVS_ROOT_DIR: &str = "intel/avs"; // loader.c:36
/// `AVS_BASEFW_FILENAME` (loader.c:37).
pub const AVS_BASEFW_FILENAME: &str = "dsp_basefw.bin"; // loader.c:37
/// `skl_desc.name` (core.c:816): the directory under [`AVS_ROOT_DIR`] a Kaby Lake part loads from.
pub const SKL_SPEC_NAME: &str = "skl"; // core.c:816

/// Slot 0 is the base firmware (loader.c:719-720).
pub const BASEFW_SLOT_NAME: [u8; AVS_LIB_NAME_SIZE] = *b"BASEFW\0\0"; // loader.c:720

/// `strncmp(a, b, AVS_LIB_NAME_SIZE) == 0` (loader.c:538): equal up to the first NUL, at most 8 bytes.
pub fn lib_names_equal(a: &[u8; AVS_LIB_NAME_SIZE], b: &[u8; AVS_LIB_NAME_SIZE]) -> bool {
    for i in 0..AVS_LIB_NAME_SIZE {
        if a[i] != b[i] {
            return false;
        }
        if a[i] == 0 {
            return true;
        }
    }
    true
}

/// What to do with one library once its manifest name is known (loader.c:536-543).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// "Don't load anything that's already in DSP memory" (loader.c:536-539).
    AlreadyLoaded,
    /// DMA it and send LOAD_LIBRARY with this id.
    Load { lib_id: u32 },
}

/// The names of the libraries the DSP holds, indexed by `lib_id` (`adev->lib_names`, loader.c:706-720).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryTable {
    names: [[u8; AVS_LIB_NAME_SIZE]; MAX_LIB_SLOTS],
    max_libs: u32,
}

/// One `avs_dsp_load_libraries` call: where its ids start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Batch {
    pub start: u32,
}

impl LibraryTable {
    /// A table with only the base firmware in slot 0. `max_libs` is the firmware's `AVS_FW_CFG_MAX_LIBS_COUNT`.
    pub fn new(max_libs: u32) -> Result<Self, FwError> {
        if max_libs as usize > MAX_LIB_SLOTS {
            return Err(FwError::LibraryIdWidth { max_libs });
        }
        let mut names = [[0u8; AVS_LIB_NAME_SIZE]; MAX_LIB_SLOTS];
        names[0] = BASEFW_SLOT_NAME;
        Ok(Self { names, max_libs })
    }

    /// Forget every library but the base firmware: "Full boot, clear cached data except for basefw (slot 0)" (loader.c:621-623).
    pub fn clear_for_full_boot(&mut self) {
        for n in self.names.iter_mut().skip(1) {
            *n = [0; AVS_LIB_NAME_SIZE];
        }
    }

    /// The first free slot (loader.c:495-497); `max_libs` when full.
    pub fn next_id(&self) -> u32 {
        let mut id = 0;
        while id < self.max_libs {
            if self.names[id as usize][0] == 0 {
                break;
            }
            id += 1;
        }
        id
    }

    /// Start a batch of `num_libs` libraries: refuses when `id + num_libs >= max_libs_count` (loader.c:498-499) - note
    /// Linux's `>=`, so one slot is always left free.
    pub fn begin(&self, num_libs: u32) -> Result<Batch, FwError> {
        let id = self.next_id();
        if id.saturating_add(num_libs) >= self.max_libs {
            return Err(FwError::LibraryTableFull { next_id: id, wanted: num_libs, max_libs: self.max_libs });
        }
        Ok(Batch { start: id })
    }

    /// Decide for a library whose manifest name is `name` (loader.c:536-543).
    pub fn decide(&self, name: &[u8; AVS_LIB_NAME_SIZE]) -> Decision {
        let id = self.next_id();
        for j in 0..id as usize {
            if lib_names_equal(&self.names[j], name) {
                return Decision::AlreadyLoaded;
            }
        }
        Decision::Load { lib_id: id }
    }

    /// Record a library the DSP accepted (`strscpy(adev->lib_names[id], man->name, ...)`, loader.c:545): only after the
    /// load succeeded, so a failed load leaves the slot free.
    pub fn record(&mut self, lib_id: u32, name: &[u8; AVS_LIB_NAME_SIZE]) {
        if (lib_id as usize) < MAX_LIB_SLOTS {
            self.names[lib_id as usize] = *name;
        }
    }

    /// `start == id ? 1 : 0` (loader.c:551): true when the batch loaded nothing new.
    pub fn nothing_new(&self, batch: Batch) -> bool {
        self.next_id() == batch.start
    }

    pub fn name(&self, lib_id: u32) -> Option<&[u8; AVS_LIB_NAME_SIZE]> {
        self.names.get(lib_id as usize)
    }
}

fn put(out: &mut [u8], at: &mut usize, s: &[u8]) -> Result<(), FwError> {
    if *at + s.len() > out.len() {
        return Err(FwError::NameBufferTooSmall { need: *at + s.len(), have: out.len() });
    }
    out[*at..*at + s.len()].copy_from_slice(s);
    *at += s.len();
    Ok(())
}

/// `"%s/%s/%s"` of [`AVS_ROOT_DIR`], [`SKL_SPEC_NAME`] and `file` (loader.c:509-510, :563). Returns the length written.
pub fn firmware_path(file: &str, out: &mut [u8]) -> Result<usize, FwError> {
    let mut at = 0;
    put(out, &mut at, AVS_ROOT_DIR.as_bytes())?;
    put(out, &mut at, b"/")?;
    put(out, &mut at, SKL_SPEC_NAME.as_bytes())?;
    put(out, &mut at, b"/")?;
    put(out, &mut at, file.as_bytes())?;
    Ok(at)
}

/// The order `%pU` prints a `guid_t`'s bytes in (little-endian GUID: the first three fields are byte-swapped).
const GUID_INDEX: [usize; 16] = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15]; // lib/uuid.c:22 (selected by lib/vsprintf.c:1748)

/// `"%s/%s/dsp_mod_%pUL.bin"` (loader.c:222-223) for the module whose `guid_t` bytes (`mentry->uuid.b`) are `uuid`.
pub fn module_path(uuid: &[u8; 16], out: &mut [u8]) -> Result<usize, FwError> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut at = 0;
    put(out, &mut at, AVS_ROOT_DIR.as_bytes())?;
    put(out, &mut at, b"/")?;
    put(out, &mut at, SKL_SPEC_NAME.as_bytes())?;
    put(out, &mut at, b"/dsp_mod_")?;
    for (n, &i) in GUID_INDEX.iter().enumerate() {
        if n == 4 || n == 6 || n == 8 || n == 10 {
            put(out, &mut at, b"-")?;
        }
        put(out, &mut at, &[HEX[(uuid[i] >> 4) as usize], HEX[(uuid[i] & 0xF) as usize]])?;
    }
    put(out, &mut at, b".bin")?;
    Ok(at)
}
