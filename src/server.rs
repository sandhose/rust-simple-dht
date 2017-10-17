use std::net::SocketAddr;
use std::io;
use futures::{Future, Stream};
use tokio_core::net::{UdpFramed, UdpSocket};
use tokio_core::reactor::Handle;


use messages::{UdpMessage, Message};
use state::ServerState;

pub fn connect(&addr: &SocketAddr,
               handle: &Handle,
               _: ServerState)
               -> Box<Stream<Item = (SocketAddr, Message), Error = io::Error>> {
    let socket = UdpSocket::bind(&addr, handle).expect("failed to bind socket");

    let (_, stream) = socket.framed(UdpMessage).split();


    Box::new(stream)
}
