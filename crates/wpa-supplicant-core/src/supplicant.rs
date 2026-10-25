// SPDX-License-Identifier: ISC
//! The supplicant state machine: 4-way handshake messages 1 and 3 in, 2 and 4 out, then the group
//! key handshake. Port of `ieee80211_eapol_key_input` and its handlers in OpenBSD
//! sys/net80211/ieee80211_pae_input.c (see the crate docs for the deliberate departures).

use crate::frame::{self, info, Build, KeyFrame, FIXED_LEN, NONCE_LEN};
use crate::install::{Entropy, GroupKey, KeyInstaller, PairwiseKey};
use crate::kde::{self, KDE_GTK, KDE_PMKID};
use crate::refusal::{Refusal, Severity};
use crate::rsne::{Akm, Cipher, Rsne, IE_MAX};
use wpa_crypto_core::kdf::{derive_ptk, Ptk};
use wpa_crypto_core::keywrap;
use wpa_crypto_core::Pmk;

/// The whole 4-way handshake must finish within this many milliseconds of the association
/// (wpa_supplicant's authentication timeout is also 10 s). Policy, tunable on silicon.
pub const HANDSHAKE_TIMEOUT_MS: u64 = 10_000;
/// Message 1s answered per negotiation; a sixth is a looping or hostile authenticator.
pub const MAX_M1_ANSWERS: u8 = 5;
/// Largest key-data field this station buffers (RSNE + GTK KDE + padding is under 100 bytes).
pub const MAX_KEY_DATA: usize = 512;
/// Largest frame this station builds: the fixed header plus one RSNE.
pub const MAX_TX: usize = FIXED_LEN + IE_MAX;

/// What the station knows from association: the addresses, the negotiated suites, and both RSNEs.
pub struct Config<'a> {
    /// Authenticator (AP) address, the BSSID.
    pub aa: [u8; 6],
    /// Supplicant (our) address.
    pub spa: [u8; 6],
    pub akm: Akm,
    pub group: Cipher,
    /// The RSNE sent in the (Re)Association Request, whole element. Message 2 carries it verbatim.
    pub own_rsne: &'a [u8],
    /// The RSNE the AP advertised in its Beacon / Probe Response. Message 3 must repeat it exactly.
    pub ap_rsne: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Associated, waiting for message 1.
    AwaitM1,
    /// Message 2 sent, waiting for message 3.
    AwaitM3,
    /// Message 4 built; waiting for the driver to confirm it left the air ([`Supplicant::tx_done`]).
    M4Sent,
    /// Keys installed; group rekeys are answered.
    Established,
    /// Over: see the refusal that ended it.
    Failed,
}

impl State {
    pub fn code(self) -> u8 {
        match self {
            State::AwaitM1 => 0,
            State::AwaitM3 => 1,
            State::M4Sent => 2,
            State::Established => 3,
            State::Failed => 4,
        }
    }
}

/// What to do with the frame just processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// Nothing to send.
    None,
    /// `tx[..n]` is an EAPOL-Key PDU to send to the AP. After the driver has transmitted it, it
    /// calls [`Supplicant::tx_done`] (or [`Supplicant::tx_failed`]).
    Send(usize),
}

struct IeBuf {
    len: usize,
    b: [u8; IE_MAX],
}

impl IeBuf {
    fn new(src: &[u8]) -> Result<IeBuf, Refusal> {
        if src.len() > IE_MAX || src.len() < 2 || src[1] as usize + 2 != src.len() {
            return Err(Refusal::BadConfig);
        }
        let mut b = [0u8; IE_MAX];
        b[..src.len()].copy_from_slice(src);
        Ok(IeBuf { len: src.len(), b })
    }
    fn get(&self) -> &[u8] {
        &self.b[..self.len]
    }
}

/// A group key as remembered, to refuse reinstalling one already in use
/// (`ieee80211_must_update_group_key`, ieee80211_pae_input.c:351).
struct GtkRec {
    tx: bool,
    rsc: u64,
    len: usize,
    key: [u8; 32],
}

impl GtkRec {
    fn new(key: &[u8], tx: bool, rsc: u64) -> GtkRec {
        let mut k = [0u8; 32];
        k[..key.len()].copy_from_slice(key);
        GtkRec { tx, rsc, len: key.len(), key: k }
    }
    fn same_key(&self, key: &[u8]) -> bool {
        self.len == key.len() && wpa_crypto_core::ct_eq(&self.key[..self.len], key)
    }
}

impl Drop for GtkRec {
    fn drop(&mut self) {
        wpa_crypto_core::wipe(&mut self.key);
    }
}

struct Pending {
    install_ptk: bool,
    prsc: u64,
    gtk: Option<(u8, GtkRec)>,
    secure: bool,
}

pub struct Supplicant {
    aa: [u8; 6],
    spa: [u8; 6],
    akm: Akm,
    group: Cipher,
    own_rsne: IeBuf,
    ap_rsne: IeBuf,
    pmk: Pmk,
    state: State,
    started_ms: u64,
    last_replay: Option<u64>,
    anonce: Option<[u8; NONCE_LEN]>,
    snonce: [u8; NONCE_LEN],
    tptk: Option<Ptk>,
    ptk: Option<Ptk>,
    new_ptk: bool,
    m1_answers: u8,
    pending: Option<Pending>,
    gtks: [Option<GtkRec>; 4],
    port_open: bool,
    eapol_version: u8,
}

impl Supplicant {
    /// Begin the handshake that follows a successful association at `now_ms`.
    pub fn new(cfg: &Config<'_>, pmk: Pmk, now_ms: u64) -> Result<Supplicant, Refusal> {
        let own = IeBuf::new(cfg.own_rsne)?;
        let ap = IeBuf::new(cfg.ap_rsne)?;
        for ie in [own.get(), ap.get()] {
            let r = Rsne::parse(ie).map_err(|_| Refusal::BadConfig)?;
            if !r.offers_akm(cfg.akm) || !r.offers_pairwise(Cipher::Ccmp) || !r.group_is(cfg.group) {
                return Err(Refusal::BadConfig);
            }
        }
        Ok(Supplicant {
            aa: cfg.aa,
            spa: cfg.spa,
            akm: cfg.akm,
            group: cfg.group,
            own_rsne: own,
            ap_rsne: ap,
            pmk,
            state: State::AwaitM1,
            started_ms: now_ms,
            last_replay: None,
            anonce: None,
            snonce: [0; NONCE_LEN],
            tptk: None,
            ptk: None,
            new_ptk: false,
            m1_answers: 0,
            pending: None,
            gtks: [None, None, None, None],
            port_open: false,
            eapol_version: 1,
        })
    }

    pub fn state(&self) -> State {
        self.state
    }
    pub fn is_established(&self) -> bool {
        self.state == State::Established
    }
    pub fn port_open(&self) -> bool {
        self.port_open
    }

    /// Advance the clock; fails the handshake when it has run past [`HANDSHAKE_TIMEOUT_MS`].
    pub fn tick(&mut self, now_ms: u64) -> Result<(), Refusal> {
        if matches!(self.state, State::AwaitM1 | State::AwaitM3 | State::M4Sent) {
            let waited = now_ms.saturating_sub(self.started_ms);
            if waited > HANDSHAKE_TIMEOUT_MS {
                self.state = State::Failed;
                return Err(Refusal::Timeout { waited_ms: waited, limit_ms: HANDSHAKE_TIMEOUT_MS });
            }
        }
        Ok(())
    }

    /// The driver reports that an issued key command failed after it was accepted.
    pub fn fail_install(&mut self) -> Refusal {
        self.state = State::Failed;
        Refusal::InstallFailed
    }

    /// The driver reports that the frame from the last [`Reply::Send`] was transmitted. After
    /// message 4 this installs the keys (PTK, then GTK) and opens the port; for any other frame it
    /// does nothing.
    pub fn tx_done<I: KeyInstaller>(&mut self, inst: &mut I) -> Result<(), Refusal> {
        if self.state != State::M4Sent {
            return Ok(());
        }
        let p = self.pending.take();
        if let Some(p) = p {
            if p.install_ptk {
                let tk = self.ptk.as_ref().map(|k| k.tk);
                let tk = match tk {
                    Some(t) => t,
                    None => return Err(self.fail_install()),
                };
                let k = PairwiseKey { cipher: Cipher::Ccmp, tk: &tk, rsc: p.prsc };
                if inst.install_ptk(&k).is_err() {
                    return Err(self.fail_install());
                }
                self.new_ptk = false;
            }
            if let Some((kid, rec)) = p.gtk {
                let k = GroupKey {
                    cipher: self.group,
                    key_id: kid,
                    key: &rec.key[..rec.len],
                    tx: rec.tx,
                    rsc: rec.rsc,
                };
                if inst.install_gtk(&k).is_err() {
                    return Err(self.fail_install());
                }
                self.gtks[kid as usize] = Some(rec);
            }
            if p.secure && !self.port_open {
                self.port_open = true;
                inst.open_port();
            }
        }
        self.state = State::Established;
        self.m1_answers = 0;
        Ok(())
    }

    /// The driver reports that message 4 could not be transmitted: wait for the AP to retry.
    pub fn tx_failed(&mut self) {
        if self.state == State::M4Sent {
            self.pending = None;
            self.state = State::AwaitM3;
        }
    }

    fn fatal(&mut self, r: Refusal) -> Refusal {
        if r.severity() == Severity::Fatal {
            self.state = State::Failed;
        }
        r
    }

    /// Process one received EAPOL PDU (the Ethernet payload of an 0x888E frame from the AP).
    pub fn on_eapol<I: KeyInstaller, E: Entropy>(
        &mut self,
        rx: &[u8],
        tx: &mut [u8],
        now_ms: u64,
        rng: &mut E,
        inst: &mut I,
    ) -> Result<Reply, Refusal> {
        if self.state == State::Failed {
            return Err(Refusal::Unexpected { state: self.state.code(), msg: 0 });
        }
        if let Err(e) = self.tick(now_ms) {
            return Err(e);
        }
        let r = self.dispatch(rx, tx, now_ms, rng, inst);
        r.map_err(|e| self.fatal(e))
    }

    fn dispatch<I: KeyInstaller, E: Entropy>(
        &mut self,
        rx: &[u8],
        tx: &mut [u8],
        now_ms: u64,
        rng: &mut E,
        inst: &mut I,
    ) -> Result<Reply, Refusal> {
        let f = KeyFrame::parse(rx)?;
        let v = f.descriptor_version();
        if !(1..=3).contains(&v) {
            return Err(Refusal::UnknownDescriptorVersion(v));
        }
        let want = self.akm.descriptor_version();
        if v != want {
            return Err(Refusal::WrongDescriptorVersion { got: v, want });
        }
        self.eapol_version = f.eapol_version().max(1);
        let inf = f.info();
        if inf & info::REQUEST != 0 {
            return Err(Refusal::RequestFrame);
        }
        if inf & info::PAIRWISE != 0 {
            match (inf & info::MIC != 0, inf & info::ACK != 0) {
                (true, true) => self.on_m3(&f, tx),
                (false, true) => self.on_m1(&f, tx, now_ms, rng),
                _ => Err(Refusal::Unexpected { state: self.state.code(), msg: 2 }),
            }
        } else if inf & info::MIC != 0 && inf & info::ACK != 0 {
            self.on_group1(&f, tx, inst)
        } else {
            Err(Refusal::Unexpected { state: self.state.code(), msg: 6 })
        }
    }

    fn check_replay(&self, got: u64) -> Result<(), Refusal> {
        match self.last_replay {
            Some(last) if got <= last => Err(Refusal::ReplayCounter { got, last }),
            _ => Ok(()),
        }
    }

    /// `ieee80211_recv_4way_msg1` + `ieee80211_send_4way_msg2`.
    fn on_m1<E: Entropy>(
        &mut self,
        f: &KeyFrame<'_>,
        tx: &mut [u8],
        now_ms: u64,
        rng: &mut E,
    ) -> Result<Reply, Refusal> {
        self.check_replay(f.replay())?;
        // A PMKID KDE, if present, must be well formed (ieee80211_pae_input.c:250-253).
        for e in kde::elements(f.key_data()) {
            if let Some((KDE_PMKID, d)) = e.kde() {
                if d.len() != 16 {
                    return Err(Refusal::PmkidKde { len: d.len() });
                }
            }
        }
        if self.m1_answers >= MAX_M1_ANSWERS {
            return Err(Refusal::TooManyMessage1 { answered: self.m1_answers });
        }
        let anonce = f.nonce();
        // The same ANonce keeps the same SNonce: the AP may already hold our first message 2.
        if self.anonce != Some(anonce) || self.tptk.is_none() {
            rng.fill(&mut self.snonce);
            self.tptk = Some(derive_ptk(
                self.akm.kdf(),
                &self.pmk,
                &self.aa,
                &self.spa,
                &anonce,
                &self.snonce,
            ));
            self.anonce = Some(anonce);
        }
        self.new_ptk = true;
        self.m1_answers += 1;
        if self.state == State::Established {
            self.started_ms = now_ms;
        }
        self.state = State::AwaitM3;
        self.pending = None;

        let kck = self.tptk.as_ref().map(|k| k.kck).unwrap_or([0; 16]);
        let n = frame::build(
            tx,
            &Build {
                eapol_version: self.eapol_version,
                info: self.akm.descriptor_version() as u16 | info::PAIRWISE | info::MIC,
                key_len: 0,
                replay: f.replay(),
                nonce: &self.snonce,
                key_data: self.own_rsne.get(),
            },
        )?;
        frame::stamp_mic(tx, n, &kck)?;
        Ok(Reply::Send(n))
    }

    /// `ieee80211_recv_4way_msg3` + `ieee80211_send_4way_msg4`.
    fn on_m3(&mut self, f: &KeyFrame<'_>, tx: &mut [u8]) -> Result<Reply, Refusal> {
        if !matches!(self.state, State::AwaitM3 | State::M4Sent | State::Established) {
            return Err(Refusal::Unexpected { state: self.state.code(), msg: 3 });
        }
        self.check_replay(f.replay())?;
        let anonce = match self.anonce {
            Some(a) => a,
            None => return Err(Refusal::Unexpected { state: self.state.code(), msg: 3 }),
        };
        if f.nonce() != anonce {
            return Err(Refusal::AnonceChanged);
        }
        let (kck, kek) = match self.tptk.as_ref() {
            Some(k) => (k.kck, k.kek),
            None => return Err(Refusal::Unexpected { state: self.state.code(), msg: 3 }),
        };
        f.check_mic(&kck)?;
        let inf = f.info();

        // Key data: unwrap when encrypted.
        let mut buf = [0u8; MAX_KEY_DATA];
        let kd = f.key_data();
        if kd.len() > MAX_KEY_DATA {
            return Err(Refusal::KeyDataTooLong { len: kd.len(), max: MAX_KEY_DATA });
        }
        buf[..kd.len()].copy_from_slice(kd);
        let plain: &[u8] = if inf & info::ENCRYPTED != 0 {
            keywrap::unwrap_in_place(&kek, &mut buf[..kd.len()]).map_err(Refusal::KeyDataDecrypt)?;
            &buf[8..kd.len()]
        } else {
            &buf[..kd.len()]
        };

        let mut rsne1: Option<&[u8]> = None;
        let mut rsne2: Option<&[u8]> = None;
        let mut gtk: Option<kde::GtkKde<'_>> = None;
        for e in kde::elements(plain) {
            if e.id == kde::ELEMID_RSN {
                if rsne1.is_none() {
                    rsne1 = Some(e.whole);
                } else if rsne2.is_none() {
                    rsne2 = Some(e.whole);
                }
            } else if let Some((KDE_GTK, d)) = e.kde() {
                gtk = kde::parse_gtk(d);
            }
        }
        let rsne1 = rsne1.ok_or(Refusal::MissingRsne)?;
        if gtk.is_some() && inf & info::ENCRYPTED == 0 {
            return Err(Refusal::GtkNotEncrypted);
        }
        if inf & info::INSTALL == 0 {
            return Err(Refusal::MissingInstall);
        }
        if rsne1 != self.ap_rsne.get() {
            return Err(Refusal::RsneMismatch);
        }
        if let Some(second) = rsne2 {
            let r = Rsne::parse(second)?;
            let ap = Rsne::parse(self.ap_rsne.get()).map_err(|_| Refusal::BadConfig)?;
            if !r.offers_akm(self.akm) || r.akm_count() != 1 || r.group != ap.group
                || r.pairwise_count() != 1 || !r.offers_pairwise(Cipher::Ccmp)
            {
                return Err(Refusal::RsneDowngrade);
            }
        }
        let want_klen = Cipher::Ccmp.key_len() as u16;
        if f.key_len() != want_klen {
            return Err(Refusal::KeyLength { got: f.key_len(), want: want_klen });
        }
        let mut pend_gtk = None;
        if let Some(g) = gtk {
            let want = self.group.key_len();
            if g.key.len() != want {
                return Err(Refusal::GtkLength { got: g.key.len(), want });
            }
            let fresh = match &self.gtks[g.key_id as usize] {
                Some(old) => !old.same_key(g.key),
                None => true,
            };
            if fresh {
                pend_gtk = Some((g.key_id, GtkRec::new(g.key, g.tx, f.rsc())));
            }
        }

        // Accepted. Message 4 echoes this counter.
        self.last_replay = Some(f.replay());
        self.ptk = self.tptk.clone();
        let n = frame::build(
            tx,
            &Build {
                eapol_version: self.eapol_version,
                info: self.akm.descriptor_version() as u16 | info::PAIRWISE | info::MIC | info::SECURE,
                key_len: 0,
                replay: f.replay(),
                nonce: &[0; NONCE_LEN],
                key_data: &[],
            },
        )?;
        frame::stamp_mic(tx, n, &kck)?;

        let install_ptk = self.new_ptk;
        // The AP's PTK RSC is only meaningful when no GTK rides along (ieee80211_pae_input.c:569).
        let prsc = if gtk.is_none() { f.rsc() } else { 0 };
        let retransmission = self.state == State::Established && !install_ptk && pend_gtk.is_none();
        if !retransmission {
            self.pending = Some(Pending {
                install_ptk,
                prsc,
                gtk: pend_gtk,
                secure: inf & info::SECURE != 0,
            });
            self.state = State::M4Sent;
        }
        Ok(Reply::Send(n))
    }

    /// `ieee80211_recv_rsn_group_msg1` + `ieee80211_send_group_msg2`.
    fn on_group1<I: KeyInstaller>(
        &mut self,
        f: &KeyFrame<'_>,
        tx: &mut [u8],
        inst: &mut I,
    ) -> Result<Reply, Refusal> {
        if self.state != State::Established {
            return Err(Refusal::Unexpected { state: self.state.code(), msg: 5 });
        }
        self.check_replay(f.replay())?;
        let (kck, kek) = match self.ptk.as_ref() {
            Some(k) => (k.kck, k.kek),
            None => return Err(Refusal::Unexpected { state: self.state.code(), msg: 5 }),
        };
        f.check_mic(&kck)?;
        let inf = f.info();
        if inf & info::ENCRYPTED == 0 {
            return Err(Refusal::GtkMissing);
        }
        let mut buf = [0u8; MAX_KEY_DATA];
        let kd = f.key_data();
        if kd.len() > MAX_KEY_DATA {
            return Err(Refusal::KeyDataTooLong { len: kd.len(), max: MAX_KEY_DATA });
        }
        buf[..kd.len()].copy_from_slice(kd);
        keywrap::unwrap_in_place(&kek, &mut buf[..kd.len()]).map_err(Refusal::KeyDataDecrypt)?;
        let plain = &buf[8..kd.len()];
        let mut gtk = None;
        for e in kde::elements(plain) {
            if let Some((KDE_GTK, d)) = e.kde() {
                gtk = kde::parse_gtk(d);
            }
        }
        let g = gtk.ok_or(Refusal::GtkMissing)?;
        let want = self.group.key_len();
        if g.key.len() != want {
            return Err(Refusal::GtkLength { got: g.key.len(), want });
        }
        let fresh = match &self.gtks[g.key_id as usize] {
            Some(old) => !old.same_key(g.key),
            None => true,
        };
        if fresh {
            let rec = GtkRec::new(g.key, g.tx, f.rsc());
            let k = GroupKey { cipher: self.group, key_id: g.key_id, key: g.key, tx: g.tx, rsc: rec.rsc };
            if inst.install_gtk(&k).is_err() {
                return Err(Refusal::InstallFailed);
            }
            self.gtks[g.key_id as usize] = Some(rec);
        }
        self.last_replay = Some(f.replay());
        let n = frame::build(
            tx,
            &Build {
                eapol_version: self.eapol_version,
                info: self.akm.descriptor_version() as u16 | info::MIC | info::SECURE,
                key_len: 0,
                replay: f.replay(),
                nonce: &[0; NONCE_LEN],
                key_data: &[],
            },
        )?;
        frame::stamp_mic(tx, n, &kck)?;
        Ok(Reply::Send(n))
    }
}
