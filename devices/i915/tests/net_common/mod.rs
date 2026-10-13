// SPDX-License-Identifier: GPL-2.0-only
//! A FAKE kernel for the connection tests: a listen endpoint that announces clients, a (request, event) pair per client with a
//! bounded event ring, client death, and a scripted or kernel-like wake order. Shared by the live-loop and parked-service tests.
//!
//! Like the real kernel it delivers messages of at most 4096 bytes (a longer one is reported oversized), reports a dead client's queued
//! messages BEFORE the death mark, and wakes the LOWEST ready endpoint first (`auto` mode).
#![allow(dead_code)]
use i915_device::conn::{Accepted, ConnId, Peer, Recv, SendOutcome, Transport, Wake};
use i915_wire::frag::{split, Feed, Joiner, Limits};
use i915_wire::Header;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

pub enum Ev {
    /// A message arrives for the DEFAULT client and the wait returns `Request` for it.
    Req(Vec<u8>),
    /// A message arrives for this client.
    To(ConnId, Vec<u8>),
    /// The wait returns this wake; time is unchanged.
    Wake(Wake),
    /// The wait returns this wake at the absolute time `.1` (a timer firing).
    At(Wake, u64),
}

pub struct Client {
    /// Messages the client sent that the device has not taken.
    pub inbox: VecDeque<Vec<u8>>,
    /// Every message the device sent this client, in order.
    pub sent: Vec<Vec<u8>>,
    /// Messages in the event ring the client has not drained.
    pub unread: usize,
    pub dead: bool,
    /// The next sends find the ring full (a count).
    pub ring_full: usize,
    pub held: bool,
    pub released: bool,
    /// The next send reports the peer dead.
    pub send_dead: bool,
}

impl Client {
    fn new() -> Client {
        Client { inbox: VecDeque::new(), sent: vec![], unread: 0, dead: false, ring_full: 0, held: false, released: false, send_dead: false }
    }
}

pub struct Net {
    pub now: u64,
    pub script: VecDeque<Ev>,
    pub announces: VecDeque<ConnId>,
    pub clients: BTreeMap<ConnId, Client>,
    pub default: Option<ConnId>,
    /// Every wait: `(deadline asked for, accepting connects)`.
    pub waits: Vec<(Option<u64>, bool)>,
    /// The clients held at each wait.
    pub held_at_wait: Vec<Vec<ConnId>>,
    pub irq: bool,
    /// The buffer length the loop handed to `recv` last time.
    pub recv_buf: usize,
    pub next: u64,
    /// With an empty script, wake like the kernel does (announce first, then the lowest ready client); else report the transport closed.
    pub auto: bool,
    pub no_room: bool,
    pub refuse_next: Option<&'static str>,
    /// Event ring depth (messages the client has not drained before a send finds it full).
    pub depth: usize,
    pub released: Vec<ConnId>,
}

impl Default for Net {
    fn default() -> Net {
        Net {
            now: 0,
            script: VecDeque::new(),
            announces: VecDeque::new(),
            clients: BTreeMap::new(),
            default: None,
            waits: vec![],
            held_at_wait: vec![],
            irq: false,
            recv_buf: 0,
            next: 1,
            auto: false,
            no_room: false,
            refuse_next: None,
            depth: usize::MAX,
            released: vec![],
        }
    }
}

#[derive(Clone, Default)]
pub struct FakeT(pub Rc<RefCell<Net>>);

impl FakeT {
    /// A client connects: it exists and its announce is queued. Returns its connection id.
    pub fn connect(&self) -> ConnId {
        let mut s = self.0.borrow_mut();
        let id = ConnId(s.next);
        s.next += 1;
        s.clients.insert(id, Client::new());
        s.announces.push_back(id);
        id
    }
    /// The client sends a whole frame as the messages `frag::split` cuts it into (a frame that does not split goes as one message).
    pub fn send_frame(&self, id: ConnId, frame: &[u8]) -> usize {
        let msgs: Vec<Vec<u8>> = match split(frame, Limits::KERNEL) {
            Ok(c) => c.map(|m| m.to_vec()).collect(),
            Err(_) => vec![frame.to_vec()],
        };
        let n = msgs.len();
        let mut s = self.0.borrow_mut();
        s.clients.get_mut(&id).expect("client").inbox.extend(msgs);
        n
    }
    pub fn send_raw(&self, id: ConnId, msg: &[u8]) {
        self.0.borrow_mut().clients.get_mut(&id).expect("client").inbox.push_back(msg.to_vec());
    }
    pub fn kill(&self, id: ConnId) {
        self.0.borrow_mut().clients.get_mut(&id).expect("client").dead = true;
    }
    /// The client drains its event ring and returns the whole frames it received (reassembled by the shared `frag::Joiner`).
    pub fn drain_frames(&self, id: ConnId) -> Vec<Vec<u8>> {
        let mut s = self.0.borrow_mut();
        let c = s.clients.get_mut(&id).expect("client");
        c.unread = 0;
        let mut out = vec![];
        let mut j = Joiner::new(Limits::KERNEL);
        let mut buf: Vec<u8> = vec![];
        for m in c.sent.drain(..) {
            match j.feed(&m) {
                Ok(Feed::Whole) => out.push(m),
                Ok(Feed::Begin { .. }) => buf = m,
                Ok(Feed::Middle) => buf.extend_from_slice(&m),
                Ok(Feed::End) => {
                    buf.extend_from_slice(&m);
                    out.push(std::mem::take(&mut buf));
                }
                Err(_) => out.push(m), // not a frame (the first generation's four-byte answer)
            }
        }
        out
    }
    pub fn messages_sent(&self, id: ConnId) -> usize {
        self.0.borrow().clients[&id].sent.len()
    }
}

impl Transport for FakeT {
    fn now_ms(&mut self) -> u64 {
        self.0.borrow().now
    }
    fn irq_armed(&self) -> bool {
        self.0.borrow().irq
    }
    fn make_room(&mut self) -> bool {
        !self.0.borrow().no_room
    }
    fn wait(&mut self, deadline_ms: Option<u64>, accept: bool) -> Wake {
        let mut s = self.0.borrow_mut();
        s.waits.push((deadline_ms, accept));
        let held: Vec<ConnId> = s.clients.iter().filter(|(_, c)| c.held).map(|(i, _)| *i).collect();
        s.held_at_wait.push(held);
        match s.script.pop_front() {
            None if s.auto => {
                if accept && !s.announces.is_empty() {
                    return Wake::Accept;
                }
                let ready = s.clients.iter().find(|(_, c)| !c.held && !c.released && (!c.inbox.is_empty() || c.dead)).map(|(i, _)| *i);
                ready.map_or(Wake::Closed, Wake::Request)
            }
            None => Wake::Closed,
            Some(Ev::Req(f)) => {
                let id = s.default.expect("a default client");
                assert!(!s.clients[&id].held, "a request was delivered to a connection the loop holds");
                s.clients.get_mut(&id).unwrap().inbox.push_back(f);
                Wake::Request(id)
            }
            Some(Ev::To(id, f)) => {
                assert!(!s.clients[&id].held, "a request was delivered to a connection the loop holds");
                s.clients.get_mut(&id).unwrap().inbox.push_back(f);
                Wake::Request(id)
            }
            Some(Ev::Wake(w)) => w,
            Some(Ev::At(w, t)) => {
                s.now = t;
                w
            }
        }
    }
    fn accept(&mut self) -> Accepted {
        let mut s = self.0.borrow_mut();
        if let Some(name) = s.refuse_next.take() {
            return Accepted::Refused(name);
        }
        match s.announces.pop_front() {
            Some(id) => Accepted::Conn(id),
            None => Accepted::Empty,
        }
    }
    fn hold(&mut self, conn: ConnId, held: bool) {
        if let Some(c) = self.0.borrow_mut().clients.get_mut(&conn) {
            c.held = held;
        }
    }
    fn recv(&mut self, conn: ConnId, buf: &mut [u8]) -> Recv {
        let mut s = self.0.borrow_mut();
        s.recv_buf = buf.len();
        let Some(c) = s.clients.get_mut(&conn) else { return Recv::PeerDead };
        if c.released {
            return Recv::PeerDead;
        }
        match c.inbox.pop_front() {
            Some(f) if f.len() > buf.len() => Recv::Oversized(f.len() as u64),
            Some(f) => {
                buf[..f.len()].copy_from_slice(&f);
                Recv::Msg(f.len())
            }
            None if c.dead => Recv::PeerDead,
            None => Recv::Empty,
        }
    }
    fn send(&mut self, conn: ConnId, msg: &[u8]) -> SendOutcome {
        let mut s = self.0.borrow_mut();
        let depth = s.depth;
        let Some(c) = s.clients.get_mut(&conn) else { return SendOutcome::PeerDead };
        if c.send_dead || c.dead || c.released {
            return SendOutcome::PeerDead;
        }
        assert!(msg.len() <= 4096, "the kernel truncates a message past EP_CAP: the device sent {} bytes", msg.len());
        if c.ring_full > 0 {
            c.ring_full -= 1;
            return SendOutcome::RingFull;
        }
        if c.unread >= depth {
            return SendOutcome::RingFull;
        }
        c.unread += 1;
        c.sent.push(msg.to_vec());
        SendOutcome::Sent
    }
    fn peer(&self, conn: ConnId) -> Option<Peer> {
        Some(Peer { req_ep: 100 + conn.0 as u16, node: 0x7000 + conn.0 })
    }
    fn release(&mut self, conn: ConnId) -> bool {
        let mut s = self.0.borrow_mut();
        s.released.push(conn);
        match s.clients.get_mut(&conn) {
            Some(c) => {
                c.released = true;
                true
            }
            None => false,
        }
    }
}

/// A request frame.
pub fn frame(op: i915_wire::Opcode, id: u64, inc: u64, payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; i915_wire::HEADER_BYTES + payload.len()];
    f[i915_wire::HEADER_BYTES..].copy_from_slice(payload);
    Header::request(op, id, inc, payload.len() as u32).encode(&mut f).unwrap();
    f
}

/// A reply, decoded.
pub struct Resp {
    pub hdr: Header,
    pub status: i915_wire::Status,
    pub errno: i32,
    pub body: Vec<u8>,
    pub payload: Vec<u8>,
}

impl Resp {
    pub fn body<B: i915_wire::msg::Body>(&self) -> B {
        assert_eq!((self.status, self.errno), (i915_wire::Status::Ok, 0), "the call failed: {:?} errno {}", self.status, self.errno);
        B::from_payload(&self.body).unwrap()
    }
    pub fn is_err(&self, e: i32) -> bool {
        self.status == i915_wire::Status::Ok && self.errno == -e
    }
}

pub fn parse(frame: &[u8]) -> Resp {
    let (hdr, p) = Header::decode(frame).unwrap();
    assert!(hdr.is_reply());
    let (status, errno) = (i915_wire::Status::from_u32(u32::from_le_bytes(p[0..4].try_into().unwrap())).unwrap(), i32::from_le_bytes(p[4..8].try_into().unwrap()));
    Resp { hdr, status, errno, body: p[8..].to_vec(), payload: p.to_vec() }
}
