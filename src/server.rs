use std::net::SocketAddr;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use futures::{Stream, Future, IntoFuture};
use tokio_core::net::{UdpFramed, UdpSocket};
use tokio_core::reactor::Handle;
use tokio_timer::Timer;


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
        let (_, stream) = self.socket.split();

        let timer = Timer::default();
        let interval = timer.interval(Duration::from_secs(1));
        let timer_state = self.state.clone();
        let timer_future = interval.for_each(move |_| {
            println!("Tick. {:?}", timer_state);
            timer_state.remove_stale();
            Ok(())
        });
        let timer_future = timer_future.map_err(|e| io::Error::from(e));

        let server_state = self.state.clone();
        let server_future = stream.for_each(move |(src, msg)| {
            println!("Got message from {}: {:?}", src, msg);
            server_state.probe(src);
            Ok(())
        });

        let combined_future = (server_future, timer_future)
            .into_future()
            .map(|_| ());

        Box::new(combined_future)
    }
}
