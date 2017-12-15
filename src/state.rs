use std::net::SocketAddr;
use std::time::{Instant, Duration};
use std::collections::HashMap;
use std::cell::RefCell;
use futures::stream::{iter_ok, Stream};

use messages::{Message, Hash, Payload};

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
        self.last_seen.elapsed() > Duration::from_secs(TTL)
    }
}

#[derive(Debug, Clone)]
struct Content {
    data: Vec<u8>,
    pushed: Instant,
}

impl Content {
    fn from_buffer(data: Vec<u8>) -> Self {
        Content {
            pushed: Instant::now(),
            data: data,
        }
    }

    fn is_stale(&self) -> bool {
        self.pushed.elapsed() > Duration::from_secs(TTL)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServerState {
    peers: RefCell<HashMap<SocketAddr, Peer>>,
    hashes: RefCell<HashMap<Hash, Content>>,
}

impl ServerState {
    /// Process a Message, returning a Stream of Messages to respond
    pub fn process(&self, msg: Message) -> Box<Stream<Item = Message, Error = ()>> {
        let opt = match msg {
            Message::Get(hash) => {
                self.get(&hash).map(|content| Message::Put(hash, Payload(content)))
            }
            Message::Put(hash, Payload(p)) => {
                self.put(&hash, p);
                Some(Message::IHave(hash))
            }
            _ => None,
        };

        // TODO: error handling
        if let Some(out) = opt {
            Box::new(iter_ok(vec![out]))
        } else {
            Box::new(iter_ok(Vec::new()))
        }
    }

    pub fn put(&self, hash: &Hash, data: Vec<u8>) {
        let mut hashes = self.hashes.borrow_mut();
        hashes.insert(*hash, Content::from_buffer(data));
    }

    pub fn get(&self, hash: &Hash) -> Option<Vec<u8>> {
        let hashes = self.hashes.borrow();
        hashes.get(hash).map(|content| content.data.clone())
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
        self.peers.borrow().keys().map(|s| (*s, Message::KeepAlive)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::ServerState;
    use futures::Async;
    use messages::{Message, Payload, Hash};

    #[test]
    fn store_hashes() {
        let state = ServerState::default();
        let hash = Hash::from_hex("0123456789abcdef").unwrap();
        let content = vec![24, 8, 42, 12];
        assert_eq!(state.get(&hash.clone()), None);

        state.put(&hash.clone(), content.clone());
        assert_eq!(state.get(&hash), Some(content));
    }

    #[test]
    fn process_messages() {
        let state = ServerState::default();
        let hash = Hash::from_hex("0123456789abcdef").unwrap();
        let content = vec![24, 8, 42, 12];

        // `Put` should yield a `IHave` message
        let mut stream = state.process(Message::Put(hash.clone(), Payload(content.clone())));
        let expected = Message::IHave(hash.clone());
        assert_eq!(stream.poll(), Ok(Async::Ready(Some(expected))));
        assert_eq!(stream.poll(), Ok(Async::Ready(None)));

        // `Get` should yield a `Put` message
        let mut stream = state.process(Message::Get(hash.clone()));
        let expected = Message::Put(hash.clone(), Payload(content.clone()));
        assert_eq!(stream.poll(), Ok(Async::Ready(Some(expected))));
        assert_eq!(stream.poll(), Ok(Async::Ready(None)));

        // `IHave` shouldn't do anything
        let mut stream = state.process(Message::IHave);
        assert_eq!(stream.poll(), Ok(Async::Ready(None)));

        // `KeepAlive` shouldn't do anything
        let mut stream = state.process(Message::KeepAlive);
        assert_eq!(stream.poll(), Ok(Async::Ready(None)));
    }
}
