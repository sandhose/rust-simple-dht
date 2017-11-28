use std::net::SocketAddr;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use futures::{Sink, Stream, Future};
use futures::stream;
use tokio_core::net::{UdpFramed, UdpSocket};
use tokio_core::reactor::Handle;
use tokio_timer::{Timer, TimerError};


use messages::UdpMessage;
use state::ServerState;

pub struct Server {
    socket: UdpFramed<UdpMessage>,
    state: Arc<ServerState>,
}

impl Server {
    pub fn from_addr(&addr: &SocketAddr, handle: &Handle) -> io::Result<Self> {
        Ok(Server {
            socket: UdpSocket::bind(&addr, handle)?.framed(UdpMessage),
            state: Arc::new(ServerState::new()),
        })
    }

    pub fn run(self) -> Box<Future<Item = (), Error = io::Error>> {
        let (sink, stream) = self.socket.split();

        let timer = Timer::default();
        let interval = timer.interval(Duration::from_secs(1));
        let timer_state = self.state.clone();
        let timer_stream = interval.map(move |_| {
                println!("Tick. {:?}", timer_state);
                timer_state.remove_stale();
                stream::iter_ok(timer_state.keep_alive())
            })
            .flatten();
        let timer_stream = timer_stream.map_err(|e: TimerError| io::Error::from(e));

        let server_state = self.state.clone();
        let server_stream = stream.filter_map(move |(src, msg)| {
            println!("Got message from {}: {:?}", src, msg);
            server_state.probe(src);
            Some((src, msg))
        });

        let combined_stream = server_stream.select(timer_stream);

        let future = sink.send_all(combined_stream).map(|_| ());

        Box::new(future)
    }
}
