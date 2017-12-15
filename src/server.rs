use std::net::SocketAddr;
use std::io;
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::collections::HashMap;
use std::cell::RefCell;
use futures::{Sink, Stream, Future};
use futures::stream;
use tokio_core::net::{UdpFramed, UdpSocket};
use tokio_core::reactor::Handle;


use messages::UdpMessage;
use state::ServerState;

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


pub struct Server {
    socket: UdpFramed<UdpMessage>,
    state: Arc<ServerState>,
    peers: Arc<RefCell<HashMap<SocketAddr, Peer>>>,
}

impl Server {
    pub fn from_addr(&addr: &SocketAddr, handle: &Handle) -> io::Result<Self> {
        Ok(Server {
            socket: UdpSocket::bind(&addr, handle)?.framed(UdpMessage),
            state: Arc::default(),
            peers: Arc::default(),
        })
    }

    pub fn run(self) -> Box<Future<Item = (), Error = io::Error>> {
        let (sink, stream) = self.socket.split();

        let peers = Arc::clone(&self.peers);
        let broadcast_stream = Arc::clone(&self.state)
            .subscribe()
            .map_err(|_| io::Error::from(io::ErrorKind::Other))
            .map(move |msg| {
                {
                    peers.borrow_mut().retain(|_, peer| !peer.is_stale());
                }

                println!("Broadcasting {:?} to {:?}", msg, peers.borrow());

                let messages = peers.borrow()
                    .keys()
                    .map(move |src| (src.clone(), msg.clone()))
                    .collect::<Vec<_>>();
                stream::iter_ok::<_, io::Error>(messages)
            })
            .flatten();


        let state = Arc::clone(&self.state);
        let peers = Arc::clone(&self.peers);
        let server_stream = stream.map(move |(src, msg)| {
                println!("Got message from {}: {:?}", src, msg);
                peers.borrow_mut().entry(src).or_insert_with(Peer::new).probe();
                state.process(msg)
                    .map(move |msg| (src, msg))
                    .map_err(|_| io::Error::from(io::ErrorKind::Other))
            })
            .flatten();

        let combined_stream = server_stream.select(broadcast_stream);

        let future = sink.send_all(combined_stream).map(|_| ());

        Box::new(future.join(self.state.run()).map(|_| ()))
    }
}
