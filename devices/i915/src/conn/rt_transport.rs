// SPDX-License-Identifier: GPL-2.0-only
//! The production [`Transport`]: the kernel's `connect()` sessions over `coralos_rt`, and `WAIT_ANY_IRQ` as the one blocking wait.
//!
//! The LISTEN endpoint receives connect announces; each announce names a private (request, event) pair the kernel minted for one client
//! and granted to this task (RECV on the request endpoint, SEND on the event endpoint). The wait set is the listen endpoint (while the
//! loop accepts) plus the request endpoint of every connection that is not held, plus the engine line if one is armed - and the
//! deadline. Nothing here polls. A dead client's request endpoint reads READY forever until [`Transport::release`] gives the pair back.
use super::transport::{Accepted, ConnId, Peer, Recv, SendOutcome, Transport, Wake};
use alloc::vec::Vec;
use coralos_rt::session::{decode_announce, ep_set, ANNOUNCE_LEN};
use coralos_rt::word::{PEER_DEAD, SEND_OK, SEND_RING_FULL, WAIT_IRQ_DENIED, WAIT_IRQ_FIRED, WAIT_TIMEOUT};

struct Row {
    id: ConnId,
    req: u16,
    evt: u16,
    /// The connecting task's node, from the announce.
    client: u64,
    held: bool,
}

pub struct RtTransport {
    /// The engine interrupt line, if the device has armed one. None today: the interrupt-enable registers are not on the write
    /// allow-list until the owner-attended silicon increment, so completion is found by the service's own timer while work is in flight.
    pub irq: Option<u32>,
    listen: u16,
    next_id: u64,
    rows: Vec<Row>,
}

impl RtTransport {
    /// A transport serving connects on `listen_ep` (the endpoint this task holds RECV on).
    pub fn new(listen_ep: u16, irq: Option<u32>) -> RtTransport {
        RtTransport { irq, listen: listen_ep, next_id: 1, rows: Vec::new() }
    }

    fn row(&self, id: ConnId) -> Option<&Row> {
        self.rows.iter().find(|r| r.id == id)
    }
}

impl Transport for RtTransport {
    fn now_ms(&mut self) -> u64 {
        coralos_rt::ticks()
    }
    fn irq_armed(&self) -> bool {
        self.irq.is_some()
    }
    fn make_room(&mut self) -> bool {
        self.rows.try_reserve(1).is_ok()
    }
    fn wait(&mut self, deadline_ms: Option<u64>, accept: bool) -> Wake {
        let listen = accept.then_some(self.listen);
        // every endpoint was checked to fit the wait set when its announce was accepted, so this cannot name a wrapped endpoint
        let (lo, hi) = ep_set(listen.into_iter().chain(self.rows.iter().filter(|r| !r.held).map(|r| r.req))).unwrap_or((0, 0));
        let now = coralos_rt::ticks();
        let timeout = deadline_ms.map_or(0, |d| d.saturating_sub(now).max(1)); // 0 = no bound
        match coralos_rt::wait_any_irq(lo, hi, timeout, self.irq.unwrap_or(0)) {
            WAIT_TIMEOUT => Wake::Deadline,
            WAIT_IRQ_FIRED => Wake::Interrupt,
            WAIT_IRQ_DENIED => {
                self.irq = None; // the line is not ours: stop asking, and let the deadline logic carry completion
                Wake::Deadline
            }
            PEER_DEAD => Wake::Deadline,
            ep if listen.map_or(false, |l| l as u64 == ep) => Wake::Accept,
            ep => match self.rows.iter().find(|r| r.req as u64 == ep) {
                Some(r) => Wake::Request(r.id),
                None => Wake::Deadline,
            },
        }
    }
    fn accept(&mut self) -> Accepted {
        let mut buf = [0u8; ANNOUNCE_LEN + 3];
        let n = coralos_rt::try_recv(self.listen as u64, &mut buf);
        if n == 0 {
            return Accepted::Empty;
        }
        if n == PEER_DEAD {
            return Accepted::Refused("LISTEN-ENDPOINT-DEAD");
        }
        if n > buf.len() as u64 {
            return Accepted::Refused("ANNOUNCE-OVERSIZED");
        }
        let Some(a) = decode_announce(&buf[..n as usize]) else {
            return Accepted::Refused("ANNOUNCE-MALFORMED");
        };
        if ep_set([a.req]).is_err() {
            // a request endpoint the wait cannot name could never be served: give the pair back
            coralos_rt::session_release(a.req, a.evt);
            return Accepted::Refused("SESSION-EP-OUT-OF-WAIT-SET");
        }
        if self.rows.len() == self.rows.capacity() {
            coralos_rt::session_release(a.req, a.evt);
            return Accepted::Refused("CONN-NOMEM");
        }
        let id = ConnId(self.next_id);
        self.next_id += 1;
        self.rows.push(Row { id, req: a.req, evt: a.evt, client: a.client, held: false });
        Accepted::Conn(id)
    }
    fn hold(&mut self, conn: ConnId, held: bool) {
        if let Some(r) = self.rows.iter_mut().find(|r| r.id == conn) {
            r.held = held;
        }
    }
    fn recv(&mut self, conn: ConnId, buf: &mut [u8]) -> Recv {
        let Some(r) = self.row(conn) else { return Recv::PeerDead };
        let n = coralos_rt::try_recv(r.req as u64, buf);
        match n {
            0 => Recv::Empty,
            PEER_DEAD => Recv::PeerDead,
            n if n <= buf.len() as u64 => Recv::Msg(n as usize),
            n if n < (1 << 32) => Recv::Oversized(n),
            n => Recv::Error(n),
        }
    }
    fn send(&mut self, conn: ConnId, msg: &[u8]) -> SendOutcome {
        let Some(r) = self.row(conn) else { return SendOutcome::PeerDead };
        match coralos_rt::send(r.evt as u64, msg) {
            SEND_OK => SendOutcome::Sent,
            SEND_RING_FULL => SendOutcome::RingFull,
            PEER_DEAD => SendOutcome::PeerDead,
            _ => SendOutcome::Denied,
        }
    }
    fn peer(&self, conn: ConnId) -> Option<Peer> {
        self.row(conn).map(|r| Peer { req_ep: r.req, node: r.client })
    }
    fn release(&mut self, conn: ConnId) -> bool {
        match self.rows.iter().position(|r| r.id == conn) {
            Some(i) => {
                let r = self.rows.remove(i);
                coralos_rt::session_release(r.req, r.evt)
            }
            None => false,
        }
    }
}
