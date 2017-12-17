extern crate clap;
extern crate env_logger;
extern crate futures;
#[macro_use]
extern crate log;
extern crate structopt;
extern crate tokio_core;

extern crate simple_dht;

use futures::stream::futures_unordered;
use futures::Stream;
use tokio_core::reactor::Core;
use structopt::StructOpt;

use simple_dht::server;
use simple_dht::state::State;
use simple_dht::client;
use simple_dht::cli;

fn main() {
    env_logger::init().unwrap();
    let args = cli::CLI::from_args();
    debug!("CLI called {:?}", args);

    let mut core = Core::new().unwrap();
    let handle = core.handle();

    match args {
        cli::CLI::Server { bind } => {
            let state = State::default();
            let mut futures: Vec<_> = bind.0
                .into_iter()
                .map(|addr| server::listen(&state, &addr, &handle))
                .collect();

            futures.push(cli::prompt(&state, &handle));
            futures.push(state.run());

            let stream = futures_unordered(futures);
            debug!("Starting event loop");
            core.run(stream.collect()).unwrap();
        }
        cli::CLI::Client { connect, command } => {
            let msg = command.to_message();
            let future = client::request(&connect.0[0], msg.clone(), &handle);
            core.run(future).unwrap();
        }
    }
}
