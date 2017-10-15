use std::str;
use std::fmt;
use std::net::{UdpSocket, ToSocketAddrs, SocketAddr};
use std::io::Result as IOResult;
use std::time::{Instant, Duration};
use std::collections::HashMap;

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

impl Server {
    /// Create a server from a bound socket
    ///
    /// # Examples
    ///
    /// ```
    /// use std::net::UdpSocket;
    /// use simple_dht::transport::Server;
    /// Server::new(UdpSocket::bind("[::]:1234").unwrap());
    /// ```
    pub fn new(socket: UdpSocket) -> Self {
        Server {
            socket: socket,
            buffer: Buffer::new(),
            peers: HashMap::new(),
        }
    }

    /// Try to create a new Server from an address
    ///
    /// # Examples
    ///
    /// ```
    /// use simple_dht::transport::Server;
    /// Server::from_address("[::]:1234").unwrap();
    /// ```
    pub fn from_address<T: ToSocketAddrs>(address: T) -> IOResult<Self> {
        let socket = UdpSocket::bind(address)?;
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

    pub fn listen(&mut self) -> IOResult<()> {
        loop {
            let (bytes, src) = self.socket.recv_from(&mut self.buffer.0)?;

            self.remove_stale();
            self.probe(src);

            self.socket.send_to(&self.buffer.0[..bytes], src)?;

            println!("Got {} bytes from {}: {:?}",
                     bytes,
                     src,
                     &self.buffer.0[..bytes]);
            println!("Peers: {:?}", self.peers);
        }
    }
}
