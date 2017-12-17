use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use futures::{Future, Sink, Stream};
use futures::sync::mpsc;
use futures::IntoFuture;
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

use state::State;
use messages::{Message, UdpMessage};

pub fn request<'a>(
    server: &SocketAddr,
    msg: Message,
    state: &'a State,
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

    let (sender, receiver) = mpsc::channel(10);

    let socket = UdpSocket::bind(&bind, handle).expect("Could not bind socket");

    let (output_sink, input_stream) = socket.framed(UdpMessage).split();

    let output_sink = output_sink.sink_map_err(|e| error!("Error sending message: {}", e));

    handle.spawn(
        sender
            .clone()
            .send((*server, msg.clone()))
            .map_err(|e| error!("Could not send message: {}", e))
            .map(|_| ()),
    );

    let recv_future = input_stream
        .for_each(move |(src, msg)| {
            let messages = state.process(msg).map(move |msg| (src, msg));
            let f = sender
                .clone()
                .sink_map_err(|e| error!("Error sending message: {}", e))
                .send_all(messages);
            handle.spawn(f.map(|_| ()));
            Ok(())
        })
        .map_err(|e| error!("Error processing message: {}", e));

    let send_future = receiver.forward(output_sink);

    if let Message::Get(hash) = msg {
        let request = state.request(hash).map(|payload| {
            println!("{}", payload);
        });

        Box::new(
            request
                .select((recv_future, send_future).into_future().map(|_| ()))
                .map(|_| ())
                .map_err(|_| ()),
        )
    } else {
        Box::new((send_future, recv_future).into_future().map(|_| ()))
    }
}
