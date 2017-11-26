use std::net::SocketAddr;
use std::time::{Instant, Duration};
use std::collections::HashMap;
use std::cell::RefCell;

static TTL: u64 = 10;

#[derive(Debug, Clone)]
struct Peer {
    last_seen: Instant,
}

impl Peer {
    fn new() -> Self {
        Peer { last_seen: Instant::now() }
    }

    fn probe(&mut self) {
        self.last_seen = Instant::now();
    }

    fn is_stale(&self) -> bool {
        self.last_seen.elapsed().clone() > Duration::from_secs(TTL)
    }
}

#[derive(Debug, Clone)]
pub struct ServerState {
    peers: RefCell<HashMap<SocketAddr, Peer>>,
}

impl ServerState {
    /// Create a server from a bound socket
    pub fn new() -> Self {
        ServerState { peers: RefCell::new(HashMap::new()) }
    }

    /// Mark a socket as active
    pub fn probe(&self, src: SocketAddr) {
        let mut peers = self.peers.borrow_mut();
        let peer = peers.entry(src).or_insert_with(Peer::new);
        peer.probe();
    }

    /// Remove peers that timed out
    pub fn remove_stale(&self) {
        let mut peers = self.peers.borrow_mut();
        peers.retain(|_, peer| !peer.is_stale());
    }
}
