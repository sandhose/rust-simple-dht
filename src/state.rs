use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use futures::{future, stream, task, Async, Future, Poll, Stream};
use futures::sync::mpsc;
use futures::sync::oneshot;
use tokio_timer::{Timer, TimerError};

use messages::{Hash, Message, Payload};

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

#[derive(Debug, Default)]
struct HashStore {
    hashes: HashMap<Hash, Content>,
}

impl HashStore {
    pub fn put(&mut self, hash: &Hash, data: Vec<u8>) {
        self.hashes.insert(*hash, Content::from_buffer(data));
    }

    pub fn get(&self, hash: &Hash) -> Option<Vec<u8>> {
        self.hashes.get(hash).map(|content| content.data.clone())
    }

    pub fn contains(&self, hash: &Hash) -> bool {
        self.hashes.contains_key(hash)
    }

    pub fn list(&self) -> Vec<&Hash> {
        self.hashes.keys().collect()
    }

    pub fn cleanup(&mut self) {
        self.hashes.retain(|_, content| !content.is_stale());
    }
}

#[derive(Default, Debug)]
struct Listeners(Vec<mpsc::Sender<Message>>);

impl Listeners {
    pub fn broadcast(&mut self, msg: &Message) -> Result<(), mpsc::TrySendError<Message>> {
        for listener in self.0.as_mut_slice() {
            listener.try_send(msg.clone())?;
        }

        Ok(())
    }

    pub fn subscribe(&mut self) -> mpsc::Receiver<Message> {
        let (sender, receiver) = mpsc::channel(1);
        self.0.push(sender);
        receiver
    }
}

#[derive(Default, Debug)]
struct Requests(HashMap<Hash, Vec<HashRequest>>);

impl Requests {
    pub fn request(&mut self, hash: Hash) -> HashRequest {
        let request = HashRequest::default();
        self.0
            .entry(hash)
            .or_insert_with(Vec::new)
            .push(request.clone());
        request
    }

    pub fn fullfill(&mut self, store: &HashStore) {
        for hash in store.list() {
            if let Some(requests) = self.0.remove(hash) {
                // FIXME: are those unwrap safe?
                let payload = Payload(store.get(hash).unwrap());
                for mut request in requests {
                    request.fullfill(payload.clone());
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct HashRequest {
    task: Arc<RefCell<Option<task::Task>>>,
    inner: Arc<RefCell<Option<Payload>>>,
}

impl HashRequest {
    pub fn fullfill(&mut self, payload: Payload) {
        if let Some(ref task) = *self.task.borrow() {
            task.notify();
        }

        *self.inner.borrow_mut() = Some(payload);
    }
}

impl Default for HashRequest {
    fn default() -> Self {
        HashRequest {
            task: Arc::from(RefCell::from(None)),
            inner: Arc::from(RefCell::from(None)),
        }
    }
}

impl Future for HashRequest {
    type Item = Payload;
    type Error = ();

    fn poll(&mut self) -> Poll<Payload, ()> {
        println!("Polled.");
        if self.task.borrow().is_none() {
            *self.task.borrow_mut() = Some(task::current());
        }

        if let Some(ref payload) = *self.inner.borrow() {
            Ok(Async::Ready(payload.clone()))
        } else {
            Ok(Async::NotReady)
        }
    }
}

#[derive(Default, Debug)]
pub struct ServerState {
    listeners: Arc<RefCell<Listeners>>,
    hashes: Arc<RefCell<HashStore>>,
    requests: Arc<RefCell<Requests>>,
}

impl ServerState {
    /// Process a Message, returning a Stream of Messages to respond
    pub fn process(&self, msg: Message) -> Box<Stream<Item = Message, Error = ()>> {
        println!("Processing msg {:?}", msg);
        let opt = match msg {
            Message::Get(hash) => {
                let req = self.request(hash.clone());
                self.requests.borrow_mut().fullfill(&self.hashes.borrow());
                return Box::new(
                    req.map(move |payload| Message::Put(hash, payload))
                        .into_stream()
                        .map_err(|e| println!("{:?}", e)),
                );
            }
            Message::Put(hash, Payload(p)) => {
                self.put(&hash, p);
                self.broadcast(&Message::IHave(hash)).unwrap();
                None
            }
            Message::Discover(_) => {
                self.broadcast(&msg).unwrap();
                None
            }
            Message::IHave(hash) if !self.contains(&hash) => Some(Message::Get(hash)),
            _ => None,
        };

        // TODO: error handling
        if let Some(msg) = opt {
            Box::new(stream::once(Ok(msg)))
        } else {
            Box::new(stream::empty())
        }
    }

    pub fn broadcast(&self, msg: &Message) -> Result<(), mpsc::TrySendError<Message>> {
        self.listeners.borrow_mut().broadcast(msg)
    }

    pub fn subscribe(&self) -> mpsc::Receiver<Message> {
        self.listeners.borrow_mut().subscribe()
    }

    pub fn request(&self, hash: Hash) -> HashRequest {
        self.requests.borrow_mut().request(hash)
    }

    pub fn put(&self, hash: &Hash, data: Vec<u8>) {
        self.hashes.borrow_mut().put(hash, data);
    }

    pub fn get(&self, hash: &Hash) -> Option<Vec<u8>> {
        self.hashes.borrow_mut().get(hash)
    }

    fn contains(&self, hash: &Hash) -> bool {
        self.hashes.borrow_mut().contains(hash)
    }

    pub fn run(&self) -> Box<Future<Item = (), Error = ()>> {
        let listeners = Arc::clone(&self.listeners);
        let hashes = Arc::clone(&self.hashes);
        let requests = Arc::clone(&self.requests);

        let timer = Timer::default();
        let interval = timer.interval(Duration::from_secs(1));
        let timer_stream = interval.map_err(TimerError::into).and_then(move |_| {
            hashes.borrow_mut().cleanup();
            requests.borrow_mut().fullfill(&hashes.borrow());
            listeners
                .borrow_mut()
                .broadcast(&Message::KeepAlive)
                .map_err(|e| println!("Could not broadcast KeepAlive: {}", e))
        });
        Box::new(timer_stream.for_each(|_| future::ok(())))
    }
}

#[cfg(test)]
mod tests {
    use super::ServerState;
    use std::str::FromStr;
    use futures::{Async, Stream};
    use messages::{Hash, Message, Payload};

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
        let mut listener = state.subscribe();

        // `Put` should yield a `IHave` message
        let mut stream = state.process(Message::Put(hash.clone(), Payload(content.clone())));
        let expected = Message::IHave(hash.clone());
        assert_eq!(listener.poll(), Ok(Async::Ready(Some(expected))));
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
