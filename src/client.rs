use std::net::SocketAddr;
use std::io;
use futures::{Sink, Stream, Future};
use futures::IntoFuture;
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

use messages::{UdpMessage, Message};

pub fn request(&server: &SocketAddr,
               msg: Message,
               handle: &Handle)
               -> Box<Future<Item = Message, Error = io::Error>> {
    // TODO: Bind on v4 or v6 depending on server address
    let bind: SocketAddr = "[::]:0".parse().unwrap();
    let socket = match UdpSocket::bind(&bind, handle) {
        Ok(s) => s,
        Err(e) => return Box::new(Err(e).into_future()),
    };

    let (sink, stream) = socket.framed(UdpMessage).split();
    let send_future = sink.send((server, msg));
    // TODO: Do not clone, and check the message somehow
    let recv_future = stream.take(1).collect().map(|v| v[0].1.clone());

    Box::new((send_future, recv_future).into_future().map(|(_, m)| m))
}
