use std::{str, fmt, io};
use std::net::{self, ToSocketAddrs, SocketAddr};
use std::io::Result as IOResult;
use std::time::{Instant, Duration};
use std::collections::HashMap;

use futures::{Future, Poll};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

static TTL: u64 = 30;

pub struct Buffer([u8; 1024]);

impl Buffer {
    fn new() -> Self {
        Buffer([0; 1024])
    }
}

impl fmt::Debug for Buffer {
    fn fmt(&self, w: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(w, "Buffer({:?}…)", &self.0[..8])
    }
}

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
pub struct Server {
    socket: UdpSocket,
    buffer: Buffer,
    peers: HashMap<SocketAddr, Peer>,
}

impl Future for Server {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        loop {
            let (bytes, src) = try_nb!(self.socket.recv_from(&mut self.buffer.0));

            self.remove_stale();
            self.probe(src);

            println!("Got {} bytes from {}: {:?}",
                     bytes,
                     src,
                     &self.buffer.0[..bytes]);
            println!("Peers: {:?}", self.peers);
        }
    }
}

impl Server {
    /// Create a server from a bound socket
    pub fn new(socket: UdpSocket) -> Self {
        Server {
            socket: socket,
            buffer: Buffer::new(),
            peers: HashMap::new(),
        }
    }

    /// Try to create a new Server from an address
    pub fn from_address<T: ToSocketAddrs>(address: &T, handle: &Handle) -> IOResult<Self> {
        let socket = net::UdpSocket::bind(address)?;
        let socket = UdpSocket::from_socket(socket, &handle)?;
        Ok(Server::new(socket))
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
