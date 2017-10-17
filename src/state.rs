use std::net::SocketAddr;
use std::time::{Instant, Duration};
use std::collections::HashMap;

static TTL: u64 = 30;

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

#[derive(Debug)]
pub struct ServerState {
    peers: HashMap<SocketAddr, Peer>,
}

impl ServerState {
    /// Create a server from a bound socket
    pub fn new() -> Self {
        ServerState { peers: HashMap::new() }
    }

    /// Mark a socket as active
    fn probe(&mut self, src: SocketAddr) {
        let peer = self.peers.entry(src).or_insert_with(Peer::new);
        peer.probe();
    }

    /// Remove peers that timed out
    fn remove_stale(&mut self) {
        self.peers.retain(|_, peer| !peer.is_stale());
    }
}
