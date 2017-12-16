extern crate futures;
extern crate rustyline;
extern crate shlex;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate tokio_core;
extern crate tokio_timer;

macro_rules! try_to_future {
    ($res:expr) => ({
        match $res {
            Ok(inner) => inner,
            Err(error) => return Box::new(future::err(error))
        }
    })
}

pub mod messages;
pub mod state;
pub mod server;
pub mod client;
pub mod cli;
