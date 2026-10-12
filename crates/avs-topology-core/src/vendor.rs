// SPDX-License-Identifier: GPL-2.0-only
//! Vendor tuple arrays (`struct snd_soc_tplg_vendor_array`, asoc.h:220-229) and the AVS helpers that walk them
//! (topology.c:35-113, :248-328): `lookup`, `lookup_next`, `entry_size` and the token dispatch loop `avs_parse_tokens`.
//!
//! An array is `{ size, type, num_elems }` followed by `num_elems` elements of one kind. `size` counts the header and all
//! elements. Linux's walkers trust `size` and `num_elems` as far as the block they were handed; two things are different here:
//! an array with `size < 12` is refused (`lookup` never advances over a zero-sized one and spins), and `num_elems` elements
//! must fit inside `size` (Linux reads past the array when they do not).
use crate::error::TplgError;

pub const SND_SOC_TPLG_TUPLE_TYPE_UUID: u32 = 0; // asoc.h:126
pub const SND_SOC_TPLG_TUPLE_TYPE_STRING: u32 = 1; // asoc.h:127
pub const SND_SOC_TPLG_TUPLE_TYPE_BOOL: u32 = 2; // asoc.h:128
pub const SND_SOC_TPLG_TUPLE_TYPE_BYTE: u32 = 3; // asoc.h:129
pub const SND_SOC_TPLG_TUPLE_TYPE_WORD: u32 = 4; // asoc.h:130
pub const SND_SOC_TPLG_TUPLE_TYPE_SHORT: u32 = 5; // asoc.h:131
/// `SNDRV_CTL_ELEM_ID_NAME_MAXLEN` (include/uapi/sound/asound.h:1119): the width of every name and string tuple.
pub const NAME_LEN: usize = 44; // asound.h:1119

/// `sizeof(struct snd_soc_tplg_vendor_array)`: size, type, num_elems.
pub const ARRAY_HEADER_SIZE: usize = 12; // asoc.h:220-223
/// `sizeof(struct snd_soc_tplg_vendor_uuid_elem)`: token, 16 bytes.
pub const UUID_ELEM_SIZE: usize = 20; // asoc.h:203-206
/// `sizeof(struct snd_soc_tplg_vendor_value_elem)`: token, value.
pub const VALUE_ELEM_SIZE: usize = 8; // asoc.h:209-212
/// `sizeof(struct snd_soc_tplg_vendor_string_elem)`: token, 44 bytes.
pub const STRING_ELEM_SIZE: usize = 48; // asoc.h:215-218

fn rd32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// A NUL-padded 44-byte name (`char name[SNDRV_CTL_ELEM_ID_NAME_MAXLEN]`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Name {
    buf: [u8; NAME_LEN],
}

impl Default for Name {
    fn default() -> Self {
        Self { buf: [0; NAME_LEN] }
    }
}

impl core::fmt::Debug for Name {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match core::str::from_utf8(self.as_bytes()) {
            Ok(s) => write!(f, "Name({s:?})"),
            Err(_) => write!(f, "Name({:?})", self.as_bytes()),
        }
    }
}

impl Name {
    /// The bytes up to the first NUL.
    pub fn as_bytes(&self) -> &[u8] {
        let n = self.buf.iter().position(|&c| c == 0).unwrap_or(NAME_LEN);
        &self.buf[..n]
    }
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(self.as_bytes()).ok()
    }
    pub fn is_empty(&self) -> bool {
        self.buf[0] == 0
    }
    /// A name from 44 raw bytes, which must hold a NUL (Linux `snprintf(val, 44, "%s", tuple->string)` would truncate an
    /// unterminated one silently; `strnlen == maxlen` is how soc-topology.c:1109-1115 treats it: invalid).
    pub fn from_field(field: &[u8; NAME_LEN], token: u32) -> Result<Self, TplgError> {
        if !field.contains(&0) {
            return Err(TplgError::StringNotTerminated { token });
        }
        let mut n = Self { buf: [0; NAME_LEN] };
        let len = field.iter().position(|&c| c == 0).unwrap_or(NAME_LEN);
        n.buf[..len].copy_from_slice(&field[..len]);
        Ok(n)
    }
    /// A name from text; refused when it does not leave room for the NUL.
    pub fn from_bytes(s: &[u8]) -> Result<Self, TplgError> {
        if s.len() >= NAME_LEN {
            return Err(TplgError::NameTooLong);
        }
        let mut n = Self { buf: [0; NAME_LEN] };
        n.buf[..s.len()].copy_from_slice(s);
        Ok(n)
    }
}

/// The receiver of a token walk: an object being filled in. Every method has a do-nothing default, because Linux ignores a token
/// no parser of the object lists (an older topology's extra tokens, e.g. token 1802, are skipped).
pub trait Sink {
    fn int(&mut self, _token: u32, _value: u32) -> Result<(), TplgError> {
        Ok(())
    }
    fn string(&mut self, _token: u32, _value: &Name) -> Result<(), TplgError> {
        Ok(())
    }
    fn uuid(&mut self, _token: u32, _value: &[u8; 16]) -> Result<(), TplgError> {
        Ok(())
    }
}

/// The header of the array at `b[0..]`, validated against `b`: at least 12 bytes, at most `b.len()`.
pub fn array_header(b: &[u8], at: usize) -> Result<(u32, u32, u32), TplgError> {
    if b.len() < ARRAY_HEADER_SIZE {
        return Err(TplgError::Truncated { what: "vendor array header", need: ARRAY_HEADER_SIZE, have: b.len() });
    }
    let size = rd32(b, 0);
    if (size as usize) < ARRAY_HEADER_SIZE {
        return Err(TplgError::ArrayTooSmall { at, size });
    }
    Ok((size, rd32(b, 4), rd32(b, 8)))
}

/// The token of an array's first element, if it has one that fits (`tuples->value[0].token`).
fn first_token(b: &[u8], size: u32, num_elems: u32) -> Option<u32> {
    if num_elems >= 1 && size as usize >= ARRAY_HEADER_SIZE + 4 && b.len() >= ARRAY_HEADER_SIZE + 4 {
        Some(rd32(b, ARRAY_HEADER_SIZE))
    } else {
        None
    }
}

/// `avs_tplg_vendor_array_lookup` (topology.c:35-60). `block_size` bytes of `b` are the block. `Ok(Some(offset))` where the first
/// array whose first token is `token` starts, `Ok(None)` for `-ENOENT`.
pub fn lookup(b: &[u8], block_size: u32, token: u32) -> Result<Option<u32>, TplgError> {
    if block_size as usize > b.len() {
        return Err(TplgError::Truncated { what: "vendor block", need: block_size as usize, have: b.len() });
    }
    let mut pos = 0usize;
    let mut left = block_size;
    while left > 0 {
        let (size, _ty, n) = array_header(&b[pos..], pos)?;
        if size > left {
            return Err(TplgError::ArrayPastBlock { at: pos, size, block: left }); // topology.c:45-46
        }
        if first_token(&b[pos..], size, n) == Some(token) {
            return Ok(Some(pos as u32));
        }
        left -= size;
        pos += size as usize;
    }
    Ok(None)
}

/// `avs_tplg_vendor_array_lookup_next` (topology.c:71-88): like [`lookup`] but starting from the array after the first.
pub fn lookup_next(b: &[u8], block_size: u32, token: u32) -> Result<Option<u32>, TplgError> {
    if block_size as usize > b.len() {
        return Err(TplgError::Truncated { what: "vendor block", need: block_size as usize, have: b.len() });
    }
    let (size, _, _) = array_header(&b[..block_size as usize], 0)?;
    if size > block_size {
        return Err(TplgError::ArrayPastBlock { at: 0, size, block: block_size }); // topology.c:78-79
    }
    Ok(lookup(&b[size as usize..], block_size - size, token)?.map(|o| o + size))
}

/// `avs_tplg_vendor_entry_size` (topology.c:100-113): where an entry that starts at `b[0]` ends - the next array whose first
/// token is `entry_id_token`, or the end of the block when there is none.
pub fn entry_size(b: &[u8], block_size: u32, entry_id_token: u32) -> Result<u32, TplgError> {
    Ok(lookup_next(b, block_size, entry_id_token)?.unwrap_or(block_size))
}

/// `avs_parse_tokens` (topology.c:279-328): walk every array of `priv_` (exactly the `priv_size` bytes) and hand each element to
/// `sink`. Refuses an array that does not fit what is left (topology.c:294-300) and an unknown tuple type.
pub fn parse_tokens<S: Sink>(sink: &mut S, priv_: &[u8]) -> Result<(), TplgError> {
    let mut pos = 0usize;
    while pos < priv_.len() {
        let rest = &priv_[pos..];
        let (size, ty, n) = array_header(rest, pos)?;
        if size as usize > rest.len() {
            return Err(TplgError::ArrayPastBlock { at: pos, size, block: rest.len() as u32 });
        }
        let arr = &rest[..size as usize];
        let (elem_size, kind) = match ty {
            SND_SOC_TPLG_TUPLE_TYPE_UUID => (UUID_ELEM_SIZE, 0),
            SND_SOC_TPLG_TUPLE_TYPE_STRING => (STRING_ELEM_SIZE, 1),
            SND_SOC_TPLG_TUPLE_TYPE_BOOL | SND_SOC_TPLG_TUPLE_TYPE_BYTE | SND_SOC_TPLG_TUPLE_TYPE_WORD | SND_SOC_TPLG_TUPLE_TYPE_SHORT => (VALUE_ELEM_SIZE, 2),
            _ => return Err(TplgError::UnknownTupleType { ty }), // topology.c:313-316
        };
        if (n as usize).checked_mul(elem_size).map_or(true, |x| x + ARRAY_HEADER_SIZE > arr.len()) {
            return Err(TplgError::TupleCountPastArray { at: pos, num_elems: n, size });
        }
        for i in 0..n as usize {
            let at = ARRAY_HEADER_SIZE + i * elem_size;
            let token = rd32(arr, at);
            match kind {
                0 => {
                    let mut u = [0u8; 16];
                    u.copy_from_slice(&arr[at + 4..at + 20]);
                    sink.uuid(token, &u)?;
                }
                1 => {
                    let mut f = [0u8; NAME_LEN];
                    f.copy_from_slice(&arr[at + 4..at + 4 + NAME_LEN]);
                    sink.string(token, &Name::from_field(&f, token)?)?;
                }
                _ => sink.int(token, rd32(arr, at + 4))?,
            }
        }
        pos += size as usize;
    }
    Ok(())
}
