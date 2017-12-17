use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::io;
use futures::{Future, Sink, Stream};
use futures::IntoFuture;
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

use messages::{Message, UdpMessage};

pub fn request(
    server: &SocketAddr,
    msg: Message,
    handle: &Handle,
) -> Box<Future<Item = Message, Error = io::Error>> {
    // Bind on either the v6 or the v4 wildcard address based on server's address
    let bind: SocketAddr = if server.is_ipv4() {
        SocketAddr::from(SocketAddrV4::new(Ipv4Addr::from(0), 0))
    } else if server.is_ipv6() {
        SocketAddr::from(SocketAddrV6::new(Ipv6Addr::from([0; 8]), 0, 0, 0))
    } else {
        panic!("Address isn't v4 nor v6")
    };

    let socket = match UdpSocket::bind(&bind, handle) {
        Ok(s) => s,
        Err(e) => return Box::new(Err(e).into_future()),
    };

    let (sink, stream) = socket.framed(UdpMessage).split();
    let send_future = sink.send((*server, msg));
    // TODO: Do not clone, and check the message somehow
    let recv_future = stream.take(1).collect().map(|v| v[0].1.clone());

    Box::new((send_future, recv_future).into_future().map(|(_, m)| m))
}
