use std::net::SocketAddr;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::cell::RefCell;
use futures::{future, stream, Future, Sink, Stream};
use futures::sync::mpsc;
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
    handle: &'a Handle,
) -> Box<Future<Item = (), Error = io::Error> + 'a> {
    let socket = try_to_future!(UdpSocket::bind(&addr, handle));

    println!("Listening on {}", try_to_future!(socket.local_addr()));

    let (sink, stream) = socket.framed(UdpMessage).split();

    let shared_peers: Arc<PeerStore> = Arc::default();

    let (sender, receiver) = mpsc::channel(10);

    let peers = Arc::clone(&shared_peers);
    let br_sender = sender.clone();
    let broadcast_stream = state
        .subscribe()
        .map_err(|_| io::Error::from(io::ErrorKind::Other))
        .for_each(move |msg| {
            peers.cleanup();

            if let Message::Discover(addr) = msg {
                peers.probe(addr);
                return Ok(());
            }

            if peers.len() > 0 {
                println!("Broadcasting {:?} to {:?}", msg, peers);
            }

            for address in peers.addresses() {
                handle.spawn(
                    br_sender
                        .clone()
                        .send((address, msg.clone()))
                        .map_err(|_| ())
                        .map(|_| ()),
                );
            }

            Ok(())
        });

    let peers = Arc::clone(&shared_peers);
    let server_stream = stream.for_each(move |(src, msg)| {
        println!("Got message from {}: {:?}", src, msg);
        peers.probe(src);
        let f = state
            .process(msg)
            .map(move |msg| (src, msg))
            .map_err(|_| ())
            .forward(sender.clone().sink_map_err(|_| ()))
            .map(|_| ());
        handle.spawn(f);
        Ok(())
    });

    Box::new(
        sink.send_all(receiver.map_err(|_| io::Error::from(io::ErrorKind::Other)))
            .join3(server_stream, broadcast_stream)
            .map(|_| ()),
    )
}
