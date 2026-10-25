// SPDX-License-Identifier: ISC
//! Key-data elements: the RSNE and the OUI 00-0F-AC key data encapsulations (KDEs) that ride in
//! EAPOL-Key key data (802.11-2020 12.7.2 Table 12-6). The walk is
//! `ieee80211_recv_4way_msg3`'s (ieee80211_pae_input.c:420-465): a truncated element ends the
//! walk, an element of an unknown type is skipped.

pub const ELEMID_RSN: u8 = 0x30; // 802.11-2020 9.4.2.1
pub const ELEMID_VENDOR: u8 = 0xdd; // 802.11-2020 9.4.2.1
pub const OUI: [u8; 3] = [0x00, 0x0f, 0xac]; // ieee80211.h:918
pub const KDE_GTK: u8 = 1; // ieee80211.h:1184
pub const KDE_PMKID: u8 = 4; // ieee80211.h:1186
pub const KDE_IGTK: u8 = 9; // ieee80211.h:1191

/// One element of the key data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Element<'a> {
    pub id: u8,
    /// The element body (after the id and length bytes).
    pub body: &'a [u8],
    /// The whole element including its two header bytes.
    pub whole: &'a [u8],
}

impl<'a> Element<'a> {
    /// The KDE type when this is a 00-0F-AC vendor element (body = OUI, type, data).
    pub fn kde(&self) -> Option<(u8, &'a [u8])> {
        if self.id == ELEMID_VENDOR && self.body.len() >= 4 && self.body[..3] == OUI {
            Some((self.body[3], &self.body[4..]))
        } else {
            None
        }
    }
}

/// Iterate the elements of `data`, stopping at the first that overruns it.
pub struct Elements<'a> {
    rest: &'a [u8],
}

pub fn elements(data: &[u8]) -> Elements<'_> {
    Elements { rest: data }
}

impl<'a> Iterator for Elements<'a> {
    type Item = Element<'a>;
    fn next(&mut self) -> Option<Element<'a>> {
        if self.rest.len() < 2 {
            return None;
        }
        let len = self.rest[1] as usize;
        if 2 + len > self.rest.len() {
            self.rest = &[];
            return None;
        }
        let whole = &self.rest[..2 + len];
        let e = Element { id: whole[0], body: &whole[2..], whole };
        self.rest = &self.rest[2 + len..];
        Some(e)
    }
}

/// A parsed GTK KDE (802.11-2020 12.7.2 Figure 12-35): key id, Tx bit, key bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GtkKde<'a> {
    pub key_id: u8,
    pub tx: bool,
    pub key: &'a [u8],
}

/// Parse the data of a GTK KDE (after OUI and type): `key id/tx byte, reserved byte, GTK`.
pub fn parse_gtk(data: &[u8]) -> Option<GtkKde<'_>> {
    if data.len() < 2 {
        return None;
    }
    Some(GtkKde { key_id: data[0] & 3, tx: data[0] & (1 << 2) != 0, key: &data[2..] })
}
