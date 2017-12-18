use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::cell::RefCell;
use futures::{Future, IntoFuture, Sink, Stream};
use futures::sync::mpsc;
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

use messages::{Message, UdpMessage};
use state::State;

static TTL: u64 = 10;

/// Store a list of known peers
#[derive(Debug, Default)]
struct PeerStore {
    peers: RefCell<HashMap<SocketAddr, Peer>>,
}

impl PeerStore {
    /// Probe a peer
    /// Returns true if the peer wasn't known before
    pub fn probe(&self, addr: SocketAddr) -> bool {
        let is_new = !self.peers.borrow().contains_key(&addr);
        self.peers
            .borrow_mut()
            .entry(addr)
            .or_insert_with(Peer::new)
            .probe();
        is_new
    }

    /// Probe a peer, and print if it was just discovered
    pub fn probe_and_announce(&self, addr: SocketAddr) {
        if self.probe(addr) {
            println!("Discovered new peer. Hi {}!", addr);
        }
    }

    /// Cleanup stale peers
    pub fn cleanup(&self) {
        self.peers.borrow_mut().retain(|_, peer| !peer.is_stale());
    }

    /// List known peers addresses
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

/// A single peer
#[derive(Debug, Clone)]
struct Peer {
    /// The last time the peer was seen
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

/// Listen to an address
/// Multiple servers sharing the same state can listen at the same time
pub fn listen<'a>(
    state: &'a State,
    addr: &SocketAddr,
    handle: &'a Handle,
) -> Box<Future<Item = (), Error = ()> + 'a> {
    // Bind the socket
    let socket = UdpSocket::bind(&addr.clone(), handle).expect("Could not bind socket");
    println!("Listening on {}", socket.local_addr().unwrap());

    // Create a framed Stream/Sink, that encodes and decodes messages
    let (output_sink, input_stream) = socket.framed(UdpMessage).split();
    let output_sink = output_sink.sink_map_err(|e| error!("Error sending message: {}", e));

    // Channel through which the messages are output
    let (sender, receiver) = mpsc::channel(10);
    let send_future = receiver.forward(output_sink);

    // The known peer list
    let shared_peers: Arc<PeerStore> = Arc::default();

    let peers = Arc::clone(&shared_peers);
    let br_sender = sender.clone();
    let broadcast_future = state.subscribe().for_each(move |msg| {
        // Broadcast messages to every known peers
        peers.cleanup(); // Cleanup stale peers before

        // If it is a DISCOVER message, do not broadcast and add peer to known peer list
        if let Message::Discover(addr) = msg {
            peers.probe_and_announce(addr);
            return Ok(());
        }

        if peers.len() > 0 {
            debug!("Broadcasting {:?} to {:?}", msg, peers);
        }

        for address in peers.addresses() {
            // Broadcast the message to everyone
            handle.spawn(
                br_sender
                    .clone()
                    .send((address, msg.clone()))
                    .map_err(|e| error!("Error broadcasting message: {}", e))
                    .map(|_| ()),
            );
        }

        Ok(())
    });

    let peers = Arc::clone(&shared_peers);
    let server_future = input_stream
        .for_each(move |(src, msg)| {
            // Process incoming messages
            debug!("Got message from {}: {:?}", src, msg);
            peers.probe_and_announce(src);
            let response = state.process(msg).map(move |msg| (src, msg));
            // send the response to the source
            let f = sender
                .clone()
                .sink_map_err(|e| error!("Error sending message: {}", e))
                .send_all(response);
            handle.spawn(f.map(|_| ()));
            Ok(())
        })
        .map_err(|e| error!("Error processing message: {}", e));

    // Run all three futures
    return Box::new(
        (send_future, server_future, broadcast_future)
            .into_future()
            .map(|_| ()),
    );
}
