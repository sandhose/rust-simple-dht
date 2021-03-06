use std::process;
use std::io;
use std::thread;
use std::iter;
use std::str::FromStr;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;

use shlex;
use structopt::StructOpt;
use futures::{Future, Sink, Stream};
use futures::sync::mpsc::channel;
use tokio_core::reactor::Handle;
use rustyline::Editor;
use rustyline::error::ReadlineError;

use state::State;
use messages::{Hash, Message, Payload};

#[derive(Debug)]
pub struct Addrs(pub Vec<SocketAddr>);

impl FromStr for Addrs {
    type Err = io::Error;
    fn from_str(src: &str) -> Result<Addrs, io::Error> {
        Ok(Addrs(src.to_socket_addrs()?.collect()))
    }
}

/// Client subcommand
/// This one is also used in server interactive prompt
#[derive(StructOpt, Debug)]
pub enum ClientCommand {
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
    #[structopt(name = "discover", display_order_raw = "3")]
    /// DISCOVER a peer
    Discover {
        /// The peer address
        address: SocketAddr,
    },
}

impl ClientCommand {
    pub fn to_message(self) -> Message {
        match self {
            ClientCommand::Get { hash } => Message::Get(hash),
            ClientCommand::Put { hash, payload } => Message::Put(hash, payload),
            ClientCommand::Discover { address } => Message::Discover(address),
        }
    }
}

/// Simple Distributed Hash Table
#[derive(StructOpt, Debug)]
pub enum CLI {
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
        #[structopt(subcommand)] command: ClientCommand,
    },
}

/// Show a prompt to directly interract with the server state, using the client subcommand
pub fn prompt<'a>(state: &'a State, handle: &'a Handle) -> Box<Future<Item = (), Error = ()> + 'a> {
    // Server responses are sent through this channel
    let (sender, receiver) = channel(1);

    let mut rl = Editor::<()>::new();
    // The prompt is spawned in a new thread because rustyline isn't futures-aware
    thread::spawn(move || loop {
        // FIXME: clean up this mess
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let app = ClientCommand::clap();
                let args = shlex::split(&line).unwrap();
                if args.len() == 0 {
                    continue;
                }
                rl.add_history_entry(&line);
                match app.get_matches_from_safe(
                    iter::once(String::from("client")).chain(args.into_iter()),
                ) {
                    Ok(matches) => {
                        sender
                            .clone()
                            .send(ClientCommand::from_clap(matches))
                            .wait()
                            .unwrap();
                    }
                    Err(e) => {
                        println!("{}", e.message);
                    }
                };
            }
            Err(ReadlineError::Interrupted) => {
                // TODO: gracefully exit
                process::exit(0);
            }
            Err(ReadlineError::Eof) => {
                // TODO: gracefully exit
                process::exit(0);
            }
            // TODO
            _ => (),
        }
    });

    let (sender2, receiver2) = channel::<Message>(10);
    // Process each messages from prompt, and pipe the response in a new channel
    let pipe_future = receiver.for_each(move |value| {
        let f = sender2
            .clone()
            .sink_map_err(|_| ())
            .send_all(state.process(value.to_message()))
            .map(|_| ());
        handle.spawn(f);
        Ok(())
    });

    // Print each responses
    let messages_future = receiver2.for_each(|msg| {
        // TODO: pretty print messages
        println!("Response: {:?}", msg);
        Ok(())
    });
    Box::new(pipe_future.join(messages_future).map(|_| ()))
}
