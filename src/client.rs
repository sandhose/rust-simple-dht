use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use futures::{Future, Sink, Stream};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

use messages::{Message, UdpMessage};

/// Send a request to a server
/// The returned future resolves when the request is fullfilled
pub fn request(
    server: &SocketAddr,
    req: Message,
    handle: &Handle,
) -> Box<Future<Item = (), Error = ()>> {
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

    // Send the message through the socket
    let send_future = output_sink
        .send((*server, req.clone()))
        .map_err(|e| error!("Could not send message: {}", e))
        .map(|_| ());

    // Wait until a valid response arrives
    // GET waits for a PUT response
    // PUT waits for a IHAVE response
    // DISCOVER waits for any response (KEEPALIVE…)
    let recv_future = input_stream
        .filter(move |&(_, ref resp)| {
            match req {
                Message::Get(hash) => {
                    if let &Message::Put(hash2, ref payload) = resp {
                        // The server answered with the hash I wanted
                        if hash == hash2 {
                            println!("{}", payload);
                            return true;
                        }
                    }
                }
                Message::Put(hash, _) => {
                    if let &Message::IHave(hash2) = resp {
                        // The server has the hash I just pushed
                        if hash == hash2 {
                            return true;
                        }
                    }
                }
                Message::Discover(_) => return true,
                _ => unimplemented!(),
            };
            false
        })
        .take(1) // I only need one valid response
        .collect()
        .map_err(|e| error!("Error processing message: {}", e));

    Box::new(recv_future.join(send_future).map(|_| ()))
}
