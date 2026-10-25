// SPDX-License-Identifier: GPL-2.0-only
//! The device's neutral vocabulary toward whoever serves wifi-manager. Field for field the shape of the
//! CoralOS private driver ABI (`coral-wifi-abi::driver`: HELLO / RADIO / SCAN and the E_SCAN_RESULT /
//! E_SCAN_DONE / E_FAULT events) so the closed wrapper's mapping is a copy, but defined here so this
//! public crate names no closed type. Nothing in this module is a wire format.

/// SSIDs are at most 32 bytes. // IEEE 802.11 SSID element; coral-wifi-abi types.rs SSID_MAX
pub const SSID_MAX: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ssid {
    pub len: u8,
    pub bytes: [u8; SSID_MAX],
}

impl Ssid {
    pub const EMPTY: Ssid = Ssid { len: 0, bytes: [0; SSID_MAX] };

    pub fn new(s: &[u8]) -> Option<Ssid> {
        if s.len() > SSID_MAX {
            return None;
        }
        let mut bytes = [0u8; SSID_MAX];
        bytes[..s.len()].copy_from_slice(s);
        Some(Ssid { len: s.len() as u8, bytes })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// What a scanned network asks of a station.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    Open,
    Wpa2Psk,
    /// An RSN/WPA network this driver cannot join (WPA3-SAE only, enterprise, WEP, TKIP-only).
    Unsupported,
}

/// One network found by a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bss {
    pub ssid: Ssid,
    pub bssid: [u8; 6],
    /// dBm.
    pub rssi: i16,
    pub channel: u8,
    pub security: Security,
}

/// What the device tells its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// One network of scan `scan_id`.
    ScanResult { scan_id: u32, bss: Bss },
    /// Scan `scan_id` ended; `complete` is false when the firmware aborted it.
    ScanDone { scan_id: u32, complete: bool },
    /// The firmware or the bus failed; `code` is for the log (a [`crate::transport::Fault`] number).
    Fault { code: u32 },
    /// The join finished: authenticated, associated and (for a protected network) the keys are installed and the
    /// controlled port is open. Data frames may flow.
    LinkUp { bssid: [u8; 6], channel: u8, aid: u16 },
    /// The link ended.
    LinkDown { reason: LinkDown },
    /// A join step failed; `stage` names it (a marker name) and `code` is the 802.11 status or reason when the AP sent one.
    JoinFailed { stage: &'static str, code: u32 },
    /// A received data frame, as an Ethernet II frame (destination, source, ethertype, payload).
    Rx(alloc::vec::Vec<u8>),
}

/// Why a link ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkDown {
    /// The AP sent a deauthentication; carries its reason code.
    Deauthenticated(u16),
    /// The AP sent a disassociation; carries its reason code.
    Disassociated(u16),
    /// The key handshake failed; carries the 802.11 reason code this station deauthenticated with.
    HandshakeFailed(u16),
    /// The host asked to leave.
    Left,
    /// The firmware or the bus failed.
    DeviceFault,
}

/// Join one network. The PMK is derived by whoever holds the passphrase; the device never sees the passphrase and
/// forgets the PMK when the handshake ends.
#[derive(Clone, PartialEq, Eq)]
pub struct JoinRequest {
    pub ssid: Ssid,
    /// Join this BSS; `None` takes the strongest one of the last scan that carries the SSID.
    pub bssid: Option<[u8; 6]>,
    pub security: Security,
    /// Present exactly when `security` is [`Security::Wpa2Psk`].
    pub pmk: Option<[u8; 32]>,
}

impl core::fmt::Debug for JoinRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The PMK is a credential: a log line shows only whether one was given.
        f.debug_struct("JoinRequest").field("ssid", &self.ssid).field("bssid", &self.bssid).field("security", &self.security).field("pmk", &self.pmk.map(|_| "<present>")).finish()
    }
}

/// What the server asks of the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Scan all channels the NVM allows; passive and active as the regulatory state permits.
    Scan { scan_id: u32 },
    /// Join a network.
    Join(JoinRequest),
    /// Leave the network.
    Leave,
}
