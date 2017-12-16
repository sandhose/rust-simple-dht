use std::net::SocketAddr;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::cell::RefCell;
use futures::{future, stream, Future, Sink, Stream};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

use messages::{Message, UdpMessage};
use state::ServerState;

static TTL: u64 = 10;

#[derive(Debug, Default)]
struct PeerStore {
    peers: RefCell<HashMap<SocketAddr, Peer>>,
}

impl PeerStore {
    pub fn probe(&self, addr: SocketAddr) -> bool {
        let is_new = !self.peers.borrow().contains_key(&addr);
        self.peers
            .borrow_mut()
            .entry(addr)
            .or_insert_with(Peer::new)
            .probe();
        is_new
    }

    pub fn cleanup(&self) {
        self.peers.borrow_mut().retain(|_, peer| !peer.is_stale());
    }

    pub fn addresses(&self) -> Vec<SocketAddr> {
        self.peers
            .borrow()
            .keys()
            .map(|addr| addr.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.peers.borrow().len()
    }
}

#[derive(Debug, Clone)]
struct Peer {
    last_seen: Instant,
}

impl Peer {
    fn new() -> Self {
        Peer {
            last_seen: Instant::now(),
        }
    }

    fn probe(&mut self) {
        self.last_seen = Instant::now();
    }

    fn is_stale(&self) -> bool {
        self.last_seen.elapsed() > Duration::from_secs(TTL)
    }
}

pub fn listen<'a>(
    state: &'a ServerState,
    addr: &SocketAddr,
    handle: &Handle,
) -> Box<Future<Item = (), Error = io::Error> + 'a> {
    let socket = match UdpSocket::bind(&addr, handle) {
        Ok(s) => s,
        Err(e) => return Box::new(future::err(e)),
    };

    // TODO: unwrap?
    println!("Listening on {}", socket.local_addr().unwrap());

    let (sink, stream) = socket.framed(UdpMessage).split();

    let shared_peers: Arc<PeerStore> = Arc::default();

    let peers = Arc::clone(&shared_peers);
    let broadcast_stream = state
        .subscribe()
        .map_err(|_| io::Error::from(io::ErrorKind::Other))
        .map(
            move |msg| -> Box<Stream<Item = (SocketAddr, Message), Error = io::Error>> {
                peers.cleanup();

                if let Message::Discover(addr) = msg {
                    peers.probe(addr);
                    return Box::new(stream::empty());
                }

                if peers.len() > 0 {
                    println!("Broadcasting {:?} to {:?}", msg, peers);
                }

                let messages = peers
                    .addresses()
                    .into_iter()
                    .map(move |src| (src, msg.clone()))
                    .collect::<Vec<_>>();
                Box::new(stream::iter_ok::<_, io::Error>(messages))
            },
        )
        .flatten();

    let peers = Arc::clone(&shared_peers);
    let server_stream = stream
        .map(move |(src, msg)| {
            println!("Got message from {}: {:?}", src, msg);
            peers.probe(src);
            state
                .process(msg)
                .map(move |msg| (src, msg))
                .map_err(|_| io::Error::from(io::ErrorKind::Other))
        })
        .flatten();

    let combined_stream = server_stream.select(broadcast_stream);

    Box::new(sink.send_all(combined_stream).map(|_| ()))
}
