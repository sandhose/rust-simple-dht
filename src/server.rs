use std::net::SocketAddr;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::cell::RefCell;
use futures::{Future, Sink, Stream};
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
) -> Box<Future<Item = (), Error = ()> + 'a> {
    let socket = UdpSocket::bind(&addr.clone(), handle).expect("Could not bind socket");

    println!("Listening on {}", socket.local_addr().unwrap());

    let (output_sink, input_stream) = socket.framed(UdpMessage).split();

    let output_sink = output_sink.sink_map_err(|e| println!("Error sending message: {}", e));

    let shared_peers: Arc<PeerStore> = Arc::default();

    let (sender, receiver) = mpsc::channel(10);

    let peers = Arc::clone(&shared_peers);
    let br_sender = sender.clone();
    let broadcast_future = state.subscribe().for_each(move |msg| {
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
                    .map_err(|e| println!("Error broadcasting mesasge: {}", e))
                    .map(|_| ()),
            );
        }

        Ok(())
    });

    let peers = Arc::clone(&shared_peers);
    let server_future = input_stream
        .for_each(move |(src, msg)| {
            println!("Got message from {}: {:?}", src, msg);
            peers.probe(src);
            let messages = state.process(msg).map(move |msg| (src, msg));
            let f = sender
                .clone()
                .sink_map_err(|e| println!("Error sending message: {}", e))
                .send_all(messages);
            handle.spawn(f.map(|_| ()));
            Ok(())
        })
        .map_err(|e| println!("Error processing message: {}", e));

    let send_future = receiver.forward(output_sink);

    return Box::new(
        send_future
            .join3(server_future, broadcast_future)
            .map(|_| ()),
    );
}
