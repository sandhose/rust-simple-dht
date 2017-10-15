#[macro_use]
extern crate clap;
extern crate tokio_core;

extern crate simple_dht;

use tokio_core::reactor::Core;
use std::net::ToSocketAddrs;
use simple_dht::messages::{Hash, Message, Payload};
use simple_dht::transport::Server;

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


    if let Some(cmd) = matches.subcommand_matches("get") {
        let msg = Message::Get(Hash::from_hex(cmd.value_of("HASH").unwrap()).unwrap());
        println!("{:?}", msg);
        println!("{:?}", msg.serialize());
    } else if let Some(cmd) = matches.subcommand_matches("put") {
        let msg = Message::Put(Hash::from_hex(cmd.value_of("HASH").unwrap()).unwrap(),
                               Payload(cmd.value_of("PAYLOAD").unwrap().as_bytes().to_vec()));
        println!("{:?}", msg);
        println!("{:?}", msg.serialize());
    } else if let Some(_) = matches.subcommand_matches("server") {
        let mut l = Core::new().unwrap();
        let handle = l.handle();
        let server = Server::from_address(&matches.value_of("CONNECT").unwrap(), &handle).unwrap();
        println!("{:?}", server);
        l.run(server).unwrap();
    }
}
