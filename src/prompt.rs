use std::io;
use std::thread;

use futures::{Future, Stream, Sink, future};
use futures::sync::mpsc::channel;
use rustyline::error::ReadlineError;
use rustyline::Editor;

use state::ServerState;

pub fn prompt(state: &ServerState) -> Box<Future<Item = (), Error = io::Error>> {
    let (sender, receiver) = channel(1);

    let mut rl = Editor::<()>::new();
    thread::spawn(move || {
        loop {
            let readline = rl.readline(">> ");
            match readline {
                Ok(line) => {
                    rl.add_history_entry(&line);
                    sender.clone().send(line).wait().unwrap();
                }
                _ => (),
            }
        }
    });

    Box::new(receiver.map_err(|_| io::Error::from(io::ErrorKind::Other)).for_each(|value| {
        println!("Got value: {:?}", value);
        future::ok(())
    }))
}
