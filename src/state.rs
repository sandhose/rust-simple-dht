use std::io;
use std::time::{Instant, Duration};
use std::collections::HashMap;
use std::cell::RefCell;
use std::sync::Arc;
use futures::stream::iter_ok;
use futures::{Stream, Future, future};
use futures::sync::mpsc::{Sender, Receiver, TrySendError, channel};
use tokio_timer::{Timer, TimerError};

use messages::{Message, Hash, Payload};

static TTL: u64 = 10;

#[derive(Debug, Clone)]
struct Content {
    data: Vec<u8>,
    pushed: Instant,
}

impl Content {
    fn from_buffer(data: Vec<u8>) -> Self {
        Content {
            pushed: Instant::now(),
            data: data,
        }
    }

    fn is_stale(&self) -> bool {
        self.pushed.elapsed() > Duration::from_secs(TTL)
    }
}

#[derive(Default)]
struct HashStore(HashMap<Hash, Content>);

impl HashStore {
    pub fn put(&mut self, hash: &Hash, data: Vec<u8>) {
        self.0.insert(*hash, Content::from_buffer(data));
    }

    pub fn get(&self, hash: &Hash) -> Option<Vec<u8>> {
        self.0.get(hash).map(|content| content.data.clone())
    }

    pub fn cleanup(&mut self) {
        self.0.retain(|_, content| !content.is_stale());
    }
}


#[derive(Default)]
struct Listeners(Vec<Sender<Message>>);

impl Listeners {
    pub fn broadcast(&mut self, msg: &Message) -> Result<(), TrySendError<Message>> {
        for listener in self.0.as_mut_slice() {
            listener.try_send(msg.clone())?;
        }

        Ok(())
    }

    pub fn subscribe(&mut self) -> Receiver<Message> {
        let (sender, receiver) = channel(1);
        self.0.push(sender);
        receiver
    }
}

#[derive(Default)]
pub struct ServerState {
    listeners: Arc<RefCell<Listeners>>,
    hashes: Arc<RefCell<HashStore>>,
}

impl ServerState {
    /// Process a Message, returning a Stream of Messages to respond
    pub fn process(&self, msg: Message) -> Box<Stream<Item = Message, Error = ()>> {
        let opt = match msg {
            Message::Get(hash) => {
                self.get(&hash).map(|content| Message::Put(hash, Payload(content)))
            }
            Message::Put(hash, Payload(p)) => {
                self.put(&hash, p);
                Some(Message::IHave(hash))
            }
            _ => None,
        };

        // TODO: error handling
        if let Some(out) = opt {
            Box::new(iter_ok(vec![out]))
        } else {
            Box::new(iter_ok(Vec::new()))
        }
    }

    pub fn broadcast(&self, msg: &Message) -> Result<(), TrySendError<Message>> {
        self.listeners.borrow_mut().broadcast(msg)
    }

    pub fn subscribe(&self) -> Receiver<Message> {
        self.listeners.borrow_mut().subscribe()
    }

    fn put(&self, hash: &Hash, data: Vec<u8>) {
        self.hashes.borrow_mut().put(hash, data);
    }

    fn get(&self, hash: &Hash) -> Option<Vec<u8>> {
        self.hashes.borrow_mut().get(hash)
    }

    pub fn run(&self) -> Box<Future<Item = (), Error = io::Error>> {
        let listeners = Arc::clone(&self.listeners);
        let hashes = Arc::clone(&self.hashes);

        let timer = Timer::default();
        let interval = timer.interval(Duration::from_secs(1));
        let timer_stream = interval.map_err(TimerError::into)
            .and_then(move |_| {
                hashes.borrow_mut().cleanup();
                listeners.borrow_mut()
                    .broadcast(&Message::KeepAlive)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            });
        Box::new(timer_stream.for_each(|_| future::ok(())))
    }
}

#[cfg(test)]
mod tests {
    use super::ServerState;
    use std::str::FromStr;
    use futures::Async;
    use messages::{Message, Payload, Hash};

    #[test]
    fn store_hashes() {
        let state = ServerState::default();
        let hash = Hash::from_str("0123456789abcdef").unwrap();
        let content = vec![24, 8, 42, 12];
        assert_eq!(state.get(&hash.clone()), None);

        state.put(&hash.clone(), content.clone());
        assert_eq!(state.get(&hash), Some(content));
    }

    #[test]
    fn process_messages() {
        let state = ServerState::default();
        let hash = Hash::from_str("0123456789abcdef").unwrap();
        let content = vec![24, 8, 42, 12];

        // `Put` should yield a `IHave` message
        let mut stream = state.process(Message::Put(hash.clone(), Payload(content.clone())));
        let expected = Message::IHave(hash.clone());
        assert_eq!(stream.poll(), Ok(Async::Ready(Some(expected))));
        assert_eq!(stream.poll(), Ok(Async::Ready(None)));

        // `Get` should yield a `Put` message
        let mut stream = state.process(Message::Get(hash.clone()));
        let expected = Message::Put(hash.clone(), Payload(content.clone()));
        assert_eq!(stream.poll(), Ok(Async::Ready(Some(expected))));
        assert_eq!(stream.poll(), Ok(Async::Ready(None)));

        // `IHave` shouldn't do anything
        let mut stream = state.process(Message::IHave(hash.clone()));
        assert_eq!(stream.poll(), Ok(Async::Ready(None)));

        // `KeepAlive` shouldn't do anything
        let mut stream = state.process(Message::KeepAlive);
        assert_eq!(stream.poll(), Ok(Async::Ready(None)));
    }
}
