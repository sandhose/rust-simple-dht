use std::net::SocketAddr;
use std::io;
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::collections::HashMap;
use std::cell::RefCell;
use futures::{Sink, Stream, Future, stream, future};
use tokio_core::net::UdpSocket;
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

pub fn listen<'a>(state: &'a ServerState,
                  addr: &SocketAddr,
                  handle: &Handle)
                  -> Box<Future<Item = (), Error = io::Error> + 'a> {
    let socket = match UdpSocket::bind(&addr, handle) {
        Ok(s) => s,
        Err(e) => return Box::new(future::err(e)),
    };

    // TODO: unwrap?
    println!("Listening on {}", socket.local_addr().unwrap());

    let (sink, stream) = socket.framed(UdpMessage).split();

    let shared_peers: Arc<RefCell<HashMap<SocketAddr, Peer>>> = Arc::default();

    let peers = Arc::clone(&shared_peers);
    let broadcast_stream = state.subscribe()
        .map_err(|_| io::Error::from(io::ErrorKind::Other))
        .map(move |msg| {
            peers.borrow_mut().retain(|_, peer| !peer.is_stale());

            let peers = peers.borrow();
            if peers.len() > 0 {
                println!("Broadcasting {:?} to {:?}", msg, peers);
            }

            let messages = peers.keys()
                .map(move |src| (src.clone(), msg.clone()))
                .collect::<Vec<_>>();
            stream::iter_ok::<_, io::Error>(messages)
        })
        .flatten();

    let peers = Arc::clone(&shared_peers);
    let server_stream = stream.map(move |(src, msg)| {
            println!("Got message from {}: {:?}", src, msg);
            peers.borrow_mut().entry(src).or_insert_with(Peer::new).probe();
            state.process(msg)
                .map(move |msg| (src, msg))
                .map_err(|_| io::Error::from(io::ErrorKind::Other))
        })
        .flatten();

    let combined_stream = server_stream.select(broadcast_stream);

    Box::new(sink.send_all(combined_stream).map(|_| ()))
}
