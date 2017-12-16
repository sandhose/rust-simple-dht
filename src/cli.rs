use std::process;
use std::io;
use std::thread;
use std::iter;
use std::str::FromStr;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;

use shlex;
use structopt::StructOpt;
use futures::{future, Future, Sink, Stream};
use futures::sync::mpsc::channel;
use rustyline::Editor;
use rustyline::error::ReadlineError;

use state::ServerState;
use messages::{Hash, Message, Payload};

// FIXME: i don't like this
#[derive(Debug)]
pub struct Addrs(pub Vec<SocketAddr>);

impl FromStr for Addrs {
    type Err = io::Error;
    fn from_str(src: &str) -> Result<Addrs, io::Error> {
        Ok(Addrs(src.to_socket_addrs()?.collect()))
    }
}

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
}

impl ClientCommand {
    pub fn to_message(self) -> Message {
        match self {
            ClientCommand::Get { hash } => Message::Get(hash),
            ClientCommand::Put { hash, payload } => Message::Put(hash, payload),
        }
    }
}

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

pub fn prompt<'a>(state: &'a ServerState) -> Box<Future<Item = (), Error = io::Error> + 'a> {
    let (sender, receiver) = channel(1);

    let mut rl = Editor::<()>::new();
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
                println!("(EOF to exit)");
            }
            Err(ReadlineError::Eof) => {
                // TODO: gracefully exit
                process::exit(0);
            }
            // TODO
            _ => (),
        }
    });

    Box::new(
        receiver
            .map(move |value| state.process(value.to_message()))
            .flatten()
            .map_err(|_| io::Error::from(io::ErrorKind::Other))
            .for_each(|value| {
                // TODO: pretty-print this.
                println!("Got response: {:?}", value);
                future::ok(())
            }),
    )
}
