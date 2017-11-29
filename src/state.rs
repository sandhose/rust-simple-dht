use std::net::SocketAddr;
use std::time::{Instant, Duration};
use std::collections::HashMap;
use std::cell::RefCell;

use messages::{Message, Hash};

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
struct Content {
    data: Vec<u8>,
    pushed: Instant,
}

impl Content {
    fn from_buffer(data: &[u8]) -> Self {
        Content {
            pushed: Instant::now(),
            data: Vec::from(data),
        }
    }

    fn is_stale(&self) -> bool {
        self.pushed.elapsed().clone() > Duration::from_secs(TTL)
    }
}

#[derive(Debug, Clone)]
pub struct ServerState {
    peers: RefCell<HashMap<SocketAddr, Peer>>,
    hashes: RefCell<HashMap<Hash, Content>>,
}

impl ServerState {
    /// Create a server from a bound socket
    pub fn new() -> Self {
        ServerState {
            peers: RefCell::new(HashMap::new()),
            hashes: RefCell::new(HashMap::new()),
        }
    }

    pub fn put(&self, hash: Hash, data: &[u8]) {
        let mut hashes = self.hashes.borrow_mut();
        hashes.insert(hash, Content::from_buffer(data));
    }

    pub fn get(&self, hash: Hash) -> Option<Vec<u8>> {
        let hashes = self.hashes.borrow();
        hashes.get(&hash).map(|content| content.data.clone())
    }

    /// Mark a socket as active
    pub fn probe_peer(&self, src: SocketAddr) {
        let mut peers = self.peers.borrow_mut();
        let peer = peers.entry(src).or_insert_with(Peer::new);
        peer.probe();
    }

    /// Remove peers that timed out
    pub fn drop_stale(&self) {
        let mut peers = self.peers.borrow_mut();
        peers.retain(|_, peer| !peer.is_stale());

        let mut hashes = self.hashes.borrow_mut();
        hashes.retain(|_, hash| !hash.is_stale());
    }

    pub fn keep_alive(&self) -> Vec<(SocketAddr, Message)> {
        self.peers.borrow().keys().map(|s| (s.clone(), Message::KeepAlive)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::ServerState;
    use messages::Hash;

    #[test]
    fn store_hashes() {
        let state = ServerState::new();
        let hash = Hash::from_hex("0123456789abcdef").unwrap();
        let content = [24, 8, 42, 12];
        assert_eq!(state.get(hash.clone()), None);

        state.put(hash.clone(), &content);
        assert_eq!(state.get(hash.clone()), Some(Vec::from(&content[..])));
    }
}
