use std::sync::Arc;
use std::cell::RefCell;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use futures::{Future, Sink, Stream};
use futures::sync::oneshot;
use futures::IntoFuture;
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

use state::State;
use messages::{Message, UdpMessage};

pub fn request<'a>(
    server: &SocketAddr,
    req: Message,
    handle: &'a Handle,
) -> Box<Future<Item = (), Error = ()> + 'a> {
    // Bind on either the v6 or the v4 wildcard address based on server's address
    let bind: SocketAddr = if server.is_ipv4() {
        SocketAddr::from(SocketAddrV4::new(Ipv4Addr::from(0), 0))
    } else if server.is_ipv6() {
        SocketAddr::from(SocketAddrV6::new(Ipv6Addr::from([0; 8]), 0, 0, 0))
    } else {
        panic!("Address isn't v4 nor v6")
    };

    let socket = UdpSocket::bind(&bind, handle).expect("Could not bind socket");

    let (output_sink, input_stream) = socket.framed(UdpMessage).split();

    let send_future = output_sink
        .send((*server, req.clone()))
        .map_err(|e| error!("Could not send message: {}", e))
        .map(|_| ());

    let recv_future = input_stream
        .filter(move |&(_, ref resp)| {
            match req {
                Message::Get(hash) => {
                    if let &Message::Put(hash2, ref payload) = resp {
                        if hash == hash2 {
                            println!("{}", payload);
                            return true;
                        }
                    }
                }
                Message::Put(hash, _) => {
                    if let &Message::IHave(hash2) = resp {
                        if hash == hash2 {
                            return true;
                        }
                    }
                }
                _ => unimplemented!(),
            };
            false
        })
        .take(1)
        .collect()
        .map_err(|e| error!("Error processing message: {}", e));

    Box::new(recv_future.join(send_future).map(|_| ()).map_err(|_| ()))
}
