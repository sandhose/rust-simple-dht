use std::net::SocketAddr;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use futures::{Sink, Stream, Future};
use futures::stream;
use tokio_core::net::{UdpFramed, UdpSocket};
use tokio_core::reactor::Handle;
use tokio_timer::{Timer, TimerError};


use messages::{UdpMessage, Message, Payload};
use state::ServerState;

pub struct Server {
    socket: UdpFramed<UdpMessage>,
    state: Arc<ServerState>,
}

impl Server {
    pub fn from_addr(&addr: &SocketAddr, handle: &Handle) -> io::Result<Self> {
        Ok(Server {
            socket: UdpSocket::bind(&addr, handle)?.framed(UdpMessage),
            state: Arc::default(),
        })
    }

    pub fn run(self) -> Box<Future<Item = (), Error = io::Error>> {
        let (sink, stream) = self.socket.split();

        let timer = Timer::default();
        let interval = timer.interval(Duration::from_secs(1));
        let state = Arc::clone(&self.state);
        let timer_stream = interval.map(move |_| {
                println!("Tick. {:?}", state);
                state.drop_stale();
                stream::iter_ok(state.keep_alive())
            })
            .flatten();
        let timer_stream = timer_stream.map_err(TimerError::into);

        let state = Arc::clone(&self.state);
        let server_stream = stream.filter_map(move |(src, msg)| {
            println!("Got message from {}: {:?}", src, msg);
            state.probe_peer(src);

            match msg {
                Message::Get(hash) => {
                    state.get(&hash)
                        .map(|content| (src, Message::Put(hash, Payload(content))))
                }
                Message::Put(hash, Payload(p)) => {
                    state.put(&hash, p);
                    Some((src, Message::IHave(hash)))
                }

                _ => None,
            }
        });

        let combined_stream = server_stream.select(timer_stream);

        let future = sink.send_all(combined_stream).map(|_| ());

        Box::new(future)
    }
}
