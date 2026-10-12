// SPDX-License-Identifier: GPL-2.0-only
//! The vendor-array walkers (topology.c:35-113, :279-328) and the token walk.
mod common;
use avs_topology_core::error::TplgError;
use avs_topology_core::vendor::*;
use common::*;

fn a(token: u32) -> Vec<u8> {
    words(&[(token, 1)])
}

#[test]
fn lookup_finds_the_first_array_whose_first_token_matches() {
    let b = cat(&[a(10), a(20), a(30), a(20)]);
    assert_eq!(lookup(&b, b.len() as u32, 10), Ok(Some(0)));
    assert_eq!(lookup(&b, b.len() as u32, 20), Ok(Some(20)));
    assert_eq!(lookup(&b, b.len() as u32, 30), Ok(Some(40)));
    assert_eq!(lookup(&b, b.len() as u32, 99), Ok(None));
}

#[test]
fn lookup_matches_only_the_first_token_of_an_array() {
    // topology.c:48-49: `tuple = tuples->value;` - only value[0]
    let b = cat(&[words(&[(1, 0), (2, 0), (3, 0)]), a(4)]);
    assert_eq!(lookup(&b, b.len() as u32, 1), Ok(Some(0)));
    assert_eq!(lookup(&b, b.len() as u32, 2), Ok(None));
    assert_eq!(lookup(&b, b.len() as u32, 4), Ok(Some(36)));
}

#[test]
fn lookup_stays_inside_its_block() {
    let b = cat(&[a(10), a(20), a(30)]);
    assert_eq!(lookup(&b, 20, 20), Ok(None), "the block is only the first array");
    assert_eq!(lookup(&b, 40, 20), Ok(Some(20)));
    // block larger than the data
    assert!(matches!(lookup(&b, 61, 99), Err(TplgError::Truncated { .. })));
    // an array larger than what is left of the block (topology.c:45-46, -EINVAL)
    assert_eq!(lookup(&b, 30, 99), Err(TplgError::ArrayPastBlock { at: 20, size: 20, block: 10 }));
    // empty block
    assert_eq!(lookup(&b, 0, 10), Ok(None));
}

#[test]
fn a_zero_or_tiny_array_is_refused_not_looped_on() {
    // Linux: `block_size -= tuples_size` with tuples_size == 0 never terminates
    let mut b = vec![0u8; 24];
    b[..4].copy_from_slice(&dw(0));
    assert_eq!(lookup(&b, 24, 5), Err(TplgError::ArrayTooSmall { at: 0, size: 0 }));
    b[..4].copy_from_slice(&dw(11));
    assert_eq!(lookup(&b, 24, 5), Err(TplgError::ArrayTooSmall { at: 0, size: 11 }));
    // 12 is the smallest: an array with no elements
    b[..4].copy_from_slice(&dw(12));
    b[12..16].copy_from_slice(&dw(0));
    assert!(lookup(&b, 12, 5).is_ok());
}

#[test]
fn an_array_without_elements_matches_nothing() {
    let mut b = Vec::new();
    b.extend_from_slice(&dw(12));
    b.extend_from_slice(&dw(WORD));
    b.extend_from_slice(&dw(0));
    b.extend_from_slice(&a(7));
    assert_eq!(lookup(&b, b.len() as u32, 7), Ok(Some(12)));
    assert_eq!(lookup(&b, b.len() as u32, 0), Ok(None));
}

#[test]
fn lookup_next_skips_the_first_array() {
    // topology.c:71-88
    let b = cat(&[a(10), a(10), a(20), a(10)]);
    assert_eq!(lookup_next(&b, b.len() as u32, 10), Ok(Some(20)), "the second 10, not the first");
    assert_eq!(lookup_next(&b, b.len() as u32, 20), Ok(Some(40)));
    assert_eq!(lookup_next(&b, b.len() as u32, 99), Ok(None));
    assert_eq!(lookup_next(&b, 20, 10), Ok(None), "a block of one array has no next");
    assert_eq!(lookup_next(&b, 19, 10), Err(TplgError::ArrayPastBlock { at: 0, size: 20, block: 19 }));
    assert!(matches!(lookup_next(&b, 0, 10), Err(TplgError::Truncated { .. })), "an empty block has no first array");
}

#[test]
fn entry_size_ends_at_the_next_entry_or_the_block() {
    // topology.c:100-113
    let b = cat(&[a(1), a(9), a(9), a(1), a(9)]);
    assert_eq!(entry_size(&b, b.len() as u32, 1), Ok(60));
    assert_eq!(entry_size(&b[60..], 40, 1), Ok(40), "the last entry runs to the end of its block");
    assert_eq!(entry_size(&b, 60, 1), Ok(60));
}

struct Collect {
    ints: Vec<(u32, u32)>,
    strs: Vec<(u32, String)>,
    uuids: Vec<(u32, [u8; 16])>,
}
impl Sink for Collect {
    fn int(&mut self, token: u32, value: u32) -> Result<(), TplgError> {
        self.ints.push((token, value));
        Ok(())
    }
    fn string(&mut self, token: u32, value: &Name) -> Result<(), TplgError> {
        self.strs.push((token, value.as_str().unwrap().to_string()));
        Ok(())
    }
    fn uuid(&mut self, token: u32, value: &[u8; 16]) -> Result<(), TplgError> {
        self.uuids.push((token, *value));
        Ok(())
    }
}
fn collect() -> Collect {
    Collect { ints: vec![], strs: vec![], uuids: vec![] }
}

#[test]
fn parse_tokens_delivers_every_element_of_every_kind() {
    let b = cat(&[
        words(&[(1, 10), (2, 20)]),
        ints(SHORT, &[(3, 30)]),
        ints(BYTE, &[(4, 40)]),
        ints(BOOL, &[(5, 1)]),
        string(6, "hello"),
        uuid(7, [9; 16]),
    ]);
    let mut c = collect();
    parse_tokens(&mut c, &b).unwrap();
    assert_eq!(c.ints, vec![(1, 10), (2, 20), (3, 30), (4, 40), (5, 1)]);
    assert_eq!(c.strs, vec![(6, "hello".to_string())]);
    assert_eq!(c.uuids, vec![(7, [9; 16])]);
}

#[test]
fn an_empty_private_area_is_nothing_to_walk() {
    let mut c = collect();
    parse_tokens(&mut c, &[]).unwrap();
    assert!(c.ints.is_empty());
}

#[test]
fn parse_tokens_refusals() {
    let mut c = collect();
    // an unknown tuple type (topology.c:313-316)
    let mut b = words(&[(1, 1)]);
    b[4..8].copy_from_slice(&dw(9));
    assert_eq!(parse_tokens(&mut c, &b), Err(TplgError::UnknownTupleType { ty: 9 }));
    // an array larger than what is left (topology.c:294-300)
    let b = words(&[(1, 1)]);
    assert_eq!(parse_tokens(&mut c, &b[..19]), Err(TplgError::ArrayPastBlock { at: 0, size: 20, block: 19 }));
    // fewer than 12 bytes of header
    assert!(matches!(parse_tokens(&mut c, &b[..11]), Err(TplgError::Truncated { .. })));
    // a zero-sized array
    assert_eq!(parse_tokens(&mut c, &[0u8; 20]), Err(TplgError::ArrayTooSmall { at: 0, size: 0 }));
    // num_elems larger than the array can hold: Linux reads past it
    let mut b = words(&[(1, 1)]);
    b[8..12].copy_from_slice(&dw(2));
    assert_eq!(parse_tokens(&mut c, &b), Err(TplgError::TupleCountPastArray { at: 0, num_elems: 2, size: 20 }));
    let mut b = words(&[(1, 1)]);
    b[8..12].copy_from_slice(&dw(u32::MAX));
    assert!(matches!(parse_tokens(&mut c, &b), Err(TplgError::TupleCountPastArray { .. })));
    // an unterminated string
    let mut s = string(6, "x");
    for x in &mut s[16..] {
        *x = b'A';
    }
    assert_eq!(parse_tokens(&mut c, &s), Err(TplgError::StringNotTerminated { token: 6 }));
}

#[test]
fn parse_tokens_stops_at_the_first_sink_error() {
    struct Fail;
    impl Sink for Fail {
        fn int(&mut self, token: u32, _: u32) -> Result<(), TplgError> {
            if token == 2 {
                Err(TplgError::NoPins)
            } else {
                Ok(())
            }
        }
    }
    let b = words(&[(1, 0), (2, 0), (3, 0)]);
    assert_eq!(parse_tokens(&mut Fail, &b), Err(TplgError::NoPins));
}

#[test]
fn tuple_element_sizes() {
    assert_eq!((ARRAY_HEADER_SIZE, UUID_ELEM_SIZE, VALUE_ELEM_SIZE, STRING_ELEM_SIZE, NAME_LEN), (12, 20, 8, 48, 44));
    assert_eq!(
        (SND_SOC_TPLG_TUPLE_TYPE_UUID, SND_SOC_TPLG_TUPLE_TYPE_STRING, SND_SOC_TPLG_TUPLE_TYPE_BOOL, SND_SOC_TPLG_TUPLE_TYPE_BYTE, SND_SOC_TPLG_TUPLE_TYPE_WORD, SND_SOC_TPLG_TUPLE_TYPE_SHORT),
        (0, 1, 2, 3, 4, 5)
    );
}

#[test]
fn names_are_nul_terminated_44_byte_fields() {
    let n = Name::from_bytes(b"ssp1p_fe").unwrap();
    assert_eq!(n.as_bytes(), b"ssp1p_fe");
    assert_eq!(n.as_str(), Some("ssp1p_fe"));
    assert!(!n.is_empty());
    assert!(Name::default().is_empty());
    // 43 characters fit, 44 do not (no room for the NUL)
    assert!(Name::from_bytes(&[b'a'; 43]).is_ok());
    assert_eq!(Name::from_bytes(&[b'a'; 44]), Err(TplgError::NameTooLong));
    let mut f = [b'x'; 44];
    assert_eq!(Name::from_field(&f, 5), Err(TplgError::StringNotTerminated { token: 5 }));
    f[43] = 0;
    assert_eq!(Name::from_field(&f, 5).unwrap().as_bytes().len(), 43);
    // bytes after the first NUL are not part of the name
    let mut g = [0u8; 44];
    g[..2].copy_from_slice(b"ab");
    g[10] = b'z';
    assert_eq!(Name::from_field(&g, 0).unwrap().as_bytes(), b"ab");
    // not UTF-8: as_str is None, the bytes remain
    let h = Name::from_bytes(&[0xFF, 0xFE]).unwrap();
    assert_eq!(h.as_str(), None);
    assert_eq!(h.as_bytes(), &[0xFF, 0xFE]);
}
