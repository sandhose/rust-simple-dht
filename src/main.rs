#[macro_use]
extern crate clap;
extern crate tokio_core;
extern crate futures;

extern crate simple_dht;

use clap::ArgMatches;
use tokio_core::reactor::Core;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use simple_dht::messages::{Hash, Message, Payload};
use simple_dht::server::Server;
use simple_dht::client;

fn valid_host(input: String) -> Result<(), String> {
    match input.as_str().to_socket_addrs() {
        Ok(_) => Ok(()),
        Err(ref e) => Err(format!("{}", e)),
    }
}

fn valid_hash(input: String) -> Result<(), String> {
    match Hash::from_hex(input.as_str()) {
        Some(_) => Ok(()),
        None => Err(String::from("invalid hash")),
    }
}

enum Args {
    Server(SocketAddr),
    Client(SocketAddr, Message),
}

impl Args {
    fn from_matches(matches: &ArgMatches) -> Option<Self> {
        let addr = matches.value_of("CONNECT")?
            .to_socket_addrs()
            .ok()?
            .next()?;

        if matches.subcommand_matches("server").is_some() {
            Some(Args::Server(addr))
        } else {
            let msg = if let Some(m) = matches.subcommand_matches("get") {
                Message::Get(Hash::from_hex(m.value_of("HASH")?)?)
            } else if let Some(m) = matches.subcommand_matches("put") {
                let hash = Hash::from_hex(m.value_of("HASH")?)?;
                let payload = Payload(m.value_of("PAYLOAD")?.as_bytes().to_vec());
                Message::Put(hash, payload)
            } else {
                return None;
            };

            Some(Args::Client(addr, msg))
        }
    }
}

fn main() {
    let matches = clap_app!((crate_name!()) =>
        (version: crate_version!())
        (author: crate_authors!("\n"))
        (about: crate_description!())
        (@setting DeriveDisplayOrder)
        (@setting SubcommandRequiredElseHelp)
        (@setting GlobalVersion)
        (@arg CONNECT: +required {valid_host} "The host:port to connect to")
        (@subcommand get =>
            (about: "GET a hash")
            (@arg HASH: +required {valid_hash} "What hash to get")
        )
        (@subcommand put =>
            (about: "PUT a hash")
            (@arg HASH: +required {valid_hash} "Which hash to put")
            (@arg PAYLOAD: +required "What data to put")
        )
        (@subcommand server =>
            (about: "Act as a server")
        )
    )
        .get_matches();

    let mut core = Core::new().unwrap();
    match Args::from_matches(&matches).unwrap() {
        Args::Server(addr) => {
            let server = Server::from_addr(&addr, &core.handle()).unwrap();
            core.run(server.run()).unwrap()
        }
        Args::Client(addr, msg) => {
            let future = client::request(&addr, msg, &core.handle());
            let resp = core.run(future).unwrap();
            println!("{:?}", resp);
        }
    }
}
