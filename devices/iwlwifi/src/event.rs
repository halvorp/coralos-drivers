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
}

/// What the server asks of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Scan all channels the NVM allows; passive and active as the regulatory state permits.
    Scan { scan_id: u32 },
}
