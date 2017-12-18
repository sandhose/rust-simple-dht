use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::cell::RefCell;
use std::sync::Arc;
use futures::{future, stream, task, Async, Future, Poll, Stream};
use futures::sync::mpsc;
use tokio_timer::{Timer, TimerError};

use messages::{Hash, Message, Payload};

/// Time to live for peers and hashes, in seconds
static TTL: u64 = 30;

/// Stores one hash content
#[derive(Debug, Clone)]
struct Content {
    /// Hash content
    data: Vec<u8>,
    /// The last time this hash was seen
    pushed: Instant,
}

impl Content {
    fn from_buffer(data: Vec<u8>) -> Self {
        Content {
            pushed: Instant::now(),
            data: data,
        }
    }

    /// Check if content is considered as stale
    fn is_stale(&self) -> bool {
        self.pushed.elapsed() > Duration::from_secs(TTL)
    }
}

/// Stores hashes
#[derive(Debug, Default)]
struct HashStore {
    hashes: HashMap<Hash, Content>,
}

impl HashStore {
    /// Put a hash inside the store
    /// Existing value will be overwritten
    pub fn put(&mut self, hash: &Hash, data: Vec<u8>) {
        self.hashes.insert(*hash, Content::from_buffer(data));
    }

    /// Try to get a hash content from the store
    pub fn get(&self, hash: &Hash) -> Option<Vec<u8>> {
        self.hashes.get(hash).map(|content| content.data.clone())
    }

    pub fn contains(&self, hash: &Hash) -> bool {
        self.hashes.contains_key(hash)
    }

    /// List known hashes
    pub fn list(&self) -> Vec<&Hash> {
        self.hashes.keys().collect()
    }

    /// Cleanup stale hashes
    pub fn cleanup(&mut self) {
        self.hashes.retain(|_, content| !content.is_stale());
    }
}

/// Stores listeners to broadcast messages
#[derive(Default, Debug)]
struct Listeners(Vec<mpsc::Sender<Message>>);

impl Listeners {
    /// Broadcast a message to all listeners
    pub fn broadcast(&mut self, msg: &Message) -> Result<(), mpsc::TrySendError<Message>> {
        for listener in self.0.as_mut_slice() {
            listener.try_send(msg.clone())?;
        }

        Ok(())
    }

    /// Subscribe to broadcast messages
    pub fn subscribe(&mut self) -> mpsc::Receiver<Message> {
        let (sender, receiver) = mpsc::channel(1);
        self.0.push(sender);
        receiver
    }
}

/// Stores pending hash requests
#[derive(Default, Debug)]
struct Requests(HashMap<Hash, Vec<HashRequest>>);

impl Requests {
    /// Request a Hash
    /// The returned HashRequest is a future that resolves with the hash payload when found
    pub fn request(&mut self, hash: Hash) -> HashRequest {
        // FIXME: timeout?
        let request = HashRequest::default();
        self.0
            .entry(hash)
            .or_insert_with(Vec::new)
            .push(request.clone());
        request
    }

    /// Fulfill requests from the store
    pub fn fulfill(&mut self, store: &HashStore) {
        for hash in store.list() {
            if let Some(requests) = self.0.remove(hash) {
                // FIXME: are those unwrap safe?
                let payload = Payload(store.get(hash).unwrap());
                for mut request in requests {
                    request.fulfill(payload.clone());
                }
            }
        }
    }
}

/// A single request
#[derive(Debug, Clone)]
pub struct HashRequest {
    // The current task in which the future is executed
    // This is needed to trigger a poll when the request is fulfilled
    task: Arc<RefCell<Option<task::Task>>>,

    // The payload, if it was fulfilled
    inner: Arc<RefCell<Option<Payload>>>,
}

impl HashRequest {
    /// Fulfill the request with a payload
    pub fn fulfill(&mut self, payload: Payload) {
        if let Some(ref task) = *self.task.borrow() {
            // tell the executor to poll this future
            task.notify();
        }

        // Save payload content
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
        if self.task.borrow().is_none() {
            // save the task, see HashRequest::fulfill
            *self.task.borrow_mut() = Some(task::current());
        }

        if let Some(ref payload) = *self.inner.borrow() {
            Ok(Async::Ready(payload.clone()))
        } else {
            Ok(Async::NotReady)
        }
    }
}

/// The server state
#[derive(Default, Debug)]
pub struct State {
    /// Listeners subscribed to broadcasts
    listeners: Arc<RefCell<Listeners>>,
    /// Where the hashes are stored
    hashes: Arc<RefCell<HashStore>>,
    /// Pending hash requests
    requests: Arc<RefCell<Requests>>,
}

impl State {
    /// Process a Message, returning a Stream of Messages to respond
    pub fn process(&self, msg: Message) -> Box<Stream<Item = Message, Error = ()>> {
        let opt = match msg {
            Message::Get(hash) => {
                info!("Message: GET {:?}", hash);
                // Request the hash from the store
                let req = self.request(hash.clone());
                // try to immediately fullfill the request
                self.requests.borrow_mut().fulfill(&self.hashes.borrow());
                // and stream it to the client
                return Box::new(
                    req.map(move |payload| Message::Put(hash, payload))
                        .into_stream()
                        .map_err(|e| error!("{:?}", e)),
                );
            }
            Message::Put(hash, Payload(p)) => {
                info!("Message: PUT {:?} [{} bytes]", hash, p.len());
                // Put the hash in the store
                self.put(&hash, p);
                self.requests.borrow_mut().fulfill(&self.hashes.borrow());
                // and broadcast a notification to everyone
                self.broadcast(&Message::IHave(hash)).unwrap();
                None
            }
            Message::Discover(addr) => {
                info!("Message: DISCOVER {}", addr);
                // The listeners *should* intercept this DISCOVER message and add the new peer to
                // their known peer list
                self.broadcast(&msg).unwrap();
                None
            }
            Message::IHave(hash) if !self.contains(&hash) => {
                info!("Message: IHAVE {:?}", hash);
                // Someone has a hash that I don't have: get it from him!
                Some(Message::Get(hash))
            }
            m => {
                warn!("Ignored message {:?}", m);
                None
            }
        };

        if let Some(msg) = opt {
            Box::new(stream::once(Ok(msg)))
        } else {
            Box::new(stream::empty())
        }
    }

    /// Broadcast a message to all listeners
    pub fn broadcast(&self, msg: &Message) -> Result<(), mpsc::TrySendError<Message>> {
        self.listeners.borrow_mut().broadcast(msg)
    }

    /// Subscribe to broadcast messages
    pub fn subscribe(&self) -> mpsc::Receiver<Message> {
        self.listeners.borrow_mut().subscribe()
    }

    /// Request a Hash
    /// The returned HashRequest is a future that resolves with the hash payload when found
    pub fn request(&self, hash: Hash) -> HashRequest {
        self.requests.borrow_mut().request(hash)
    }

    /// Put a hash inside the store
    /// Existing value will be overwritten
    pub fn put(&self, hash: &Hash, data: Vec<u8>) {
        self.hashes.borrow_mut().put(hash, data);
    }

    /// Try to get a hash content from the store
    pub fn get(&self, hash: &Hash) -> Option<Vec<u8>> {
        self.hashes.borrow_mut().get(hash)
    }

    fn contains(&self, hash: &Hash) -> bool {
        self.hashes.borrow_mut().contains(hash)
    }

    /// Run the server loop
    pub fn run(&self) -> Box<Future<Item = (), Error = ()>> {
        debug!("Starting server loop");
        let listeners = Arc::clone(&self.listeners);
        let hashes = Arc::clone(&self.hashes);
        let requests = Arc::clone(&self.requests);

        let timer = Timer::default();
        let interval = timer.interval(Duration::from_secs(1));
        let timer_stream = interval.map_err(TimerError::into).and_then(move |_| {
            // This is run every second
            debug!("Tick.");
            hashes.borrow_mut().cleanup(); // Cleanup stale hashes
            requests.borrow_mut().fulfill(&hashes.borrow()); // Fulfill pending requests
            listeners // and broadcast KeepAlive to everyone
                .borrow_mut()
                .broadcast(&Message::KeepAlive)
                .map_err(|e| error!("Could not broadcast KeepAlive: {}", e))
        });
        Box::new(timer_stream.for_each(|_| future::ok(())))
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use std::str::FromStr;
    use futures::{Async, Stream};
    use messages::{Hash, Message};

    #[test]
    fn store_hashes() {
        let state = State::default();
        let hash = Hash::from_str("0123456789abcdef").unwrap();
        let content = vec![24, 8, 42, 12];
        assert_eq!(state.get(&hash.clone()), None);

        state.put(&hash.clone(), content.clone());
        assert_eq!(state.get(&hash), Some(content));
    }

    #[test]
    fn process_messages() {
        let state = State::default();
        let hash = Hash::from_str("0123456789abcdef").unwrap();

        /* FIXME: The HashRequest needs an Executor for testing
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
        */

        // `IHave` shouldn't do anything
        let mut stream = state.process(Message::IHave(hash.clone()));
        let expected = Message::Get(hash.clone());
        assert_eq!(stream.poll(), Ok(Async::Ready(Some(expected))));

        // `KeepAlive` shouldn't do anything
        let mut stream = state.process(Message::KeepAlive);
        assert_eq!(stream.poll(), Ok(Async::Ready(None)));
    }
}
