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
    // Use RUST_LOG env variable to set log level
    env_logger::init().unwrap();
    // Args are parsed using structopt
    // see src/cli.rs
    let args = cli::CLI::from_args();
    debug!("CLI called {:?}", args);

    // Create event loop
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    match args {
        cli::CLI::Server { bind } => {
            // Create state…
            let state = State::default();
            // …listen on addresses…
            let mut futures: Vec<_> = bind.0
                .into_iter()
                .map(|addr| server::listen(&state, &addr, &handle))
                .collect();

            // …show interactive prompt…
            futures.push(cli::prompt(&state, &handle));
            // …and run the state loop.
            futures.push(state.run());

            // Run all futures
            let stream = futures_unordered(futures);
            debug!("Starting event loop");
            core.run(stream.collect()).unwrap();
        }
        cli::CLI::Client { connect, command } => {
            // Get Message structure from command line arguments
            let msg = command.to_message();
            // TODO: Timeout? Try all addresses?
            let future = client::request(&connect.0[0], msg.clone(), &handle);
            core.run(future).unwrap();
        }
    }
}
