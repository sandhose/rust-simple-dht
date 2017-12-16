extern crate clap;
extern crate tokio_core;
extern crate futures;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;

extern crate simple_dht;

use futures::stream::futures_unordered;
use futures::Stream;
use tokio_core::reactor::Core;
use std::str::FromStr;
use std::iter;
use std::io;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use structopt::StructOpt;

use simple_dht::messages::{Hash, Message, Payload};
use simple_dht::server;
use simple_dht::state::ServerState;
use simple_dht::client;
use simple_dht::prompt;


// FIXME: i don't like this
#[derive(Debug)]
pub struct Addrs(Vec<SocketAddr>);

impl FromStr for Addrs {
    type Err = io::Error;
    fn from_str(src: &str) -> Result<Addrs, io::Error> {
        Ok(Addrs(src.to_socket_addrs()?.collect()))
    }
}


#[derive(StructOpt, Debug)]
enum ClientCommand {
    #[structopt(name = "get", display_order_raw = "1")]
    /// GET a hash
    Get {
        /// The hash to get
        hash: Hash,
    },
    #[structopt(name = "put", display_order_raw = "2")]
    /// PUT a hash
    Put {
        /// The hash to put
        hash: Hash,
        /// The payload to send
        payload: Payload,
    },
}

impl ClientCommand {
    fn to_message(self) -> Message {
        match self {
            ClientCommand::Get { hash } => Message::Get(hash),
            ClientCommand::Put { hash, payload } => Message::Put(hash, payload),
        }
    }
}

#[derive(StructOpt, Debug)]
enum CLI {
    #[structopt(name = "server")]
    /// Act as a server
    Server {
        #[structopt(default_value = "[::]:0")]
        /// The address the server should listen to
        bind: Addrs,
    },
    #[structopt(name = "client")]
    /// Send a request to a server
    Client {
        /// The host:port to connect to
        connect: Addrs,
        #[structopt(subcommand)]
        command: ClientCommand,
    },
}

fn main() {
    let args = CLI::from_args();
    println!("{:?}", args);

    let mut core = Core::new().unwrap();
    match args {
        CLI::Server { bind } => {
            let handle = core.handle();
            let state = ServerState::default();
            let server_futures = bind.0
                .into_iter()
                .map(|addr| server::listen(&state, &addr, &handle));
            let prompt_future = prompt::prompt(&state);
            let state_future = state.run();
            let stream = futures_unordered(server_futures.chain(iter::once(prompt_future))
                .chain(iter::once(state_future)));
            core.run(stream.collect()).unwrap();
        }
        CLI::Client { connect, command } => {
            // FIXME: try multiple addresses?
            let future = client::request(&connect.0[0], command.to_message(), &core.handle());
            let resp = core.run(future).unwrap();
            println!("{:?}", resp);
        }
    }
}
