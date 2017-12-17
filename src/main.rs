extern crate clap;
extern crate futures;
extern crate structopt;
extern crate tokio_core;

extern crate simple_dht;

use futures::stream::futures_unordered;
use futures::Stream;
use tokio_core::reactor::Core;
use structopt::StructOpt;
use std::iter;

use simple_dht::server;
use simple_dht::state::ServerState;
use simple_dht::client;
use simple_dht::cli;

fn main() {
    let args = cli::CLI::from_args();
    println!("{:?}", args);

    let mut core = Core::new().unwrap();
    match args {
        cli::CLI::Server { bind } => {
            let handle = core.handle();
            let state = ServerState::default();
            let server_futures = bind.0
                .into_iter()
                .map(|addr| server::listen(&state, &addr, &handle));
            let prompt_future = cli::prompt(&state, &handle);
            let state_future = state.run();
            let stream = futures_unordered(
                server_futures
                    .chain(iter::once(prompt_future))
                    .chain(iter::once(state_future)),
            );
            core.run(stream.collect()).unwrap();
        }
        cli::CLI::Client { connect, command } => {
            // FIXME: try multiple addresses?
            let future = client::request(&connect.0[0], command.to_message(), &core.handle());
            let resp = core.run(future).unwrap();
            println!("{:?}", resp);
        }
    }
}
