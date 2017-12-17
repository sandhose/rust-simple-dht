use futures::{Async, Poll, Stream};
use std::sync::Arc;
use std::cell::RefCell;

macro_rules! try_to_future {
    ($res:expr) => ({
        match $res {
            Ok(inner) => inner,
            Err(error) => return Box::new(future::err(error))
        }
    })
}

/*
pub struct Combine<S>
where
    S: Stream,
{
    stream: S,
    pool: Vec<Arc<RefCell<S::Item>>>,
}

impl<S: Stream> Combine<S> {
    fn new(s: S) -> Combine<S>
    where
        S: Stream,
        S::Item: Stream,
        <S::Item as Stream>::Error: From<S::Error>,
    {
        Combine {
            stream: s,
            pool: Vec::new(),
        }
    }
}

impl<S> Stream for Combine<S>
where
    S: Stream,
    S::Item: Stream,
    <S::Item as Stream>::Error: From<S::Error>,
{
    type Item = <S::Item as Stream>::Item;
    type Error = <S::Item as Stream>::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        /*
        loop {
            if let Some(s) = try_ready!(self.stream.poll()) {
                self.pool.push(Arc::new(s));
            }
        }
        */

        let mut to_remove = Vec::new();

        let mut iter = self.pool.clone().iter().enumerate();
        let value = loop {
            if let Some((index, stream)) = iter.next() {
                match stream.borrow_mut().poll() {
                    Ok(Async::NotReady) => (),
                    Ok(Async::Ready(None)) => to_remove.push(index),
                    other => break other,
                }
            } else {
                break Ok(Async::Ready(None));
            }
        };

        for (index, to_remove) in to_remove.iter().enumerate() {
            self.pool.remove(to_remove - index);
        }

        return value;
    }
}
*/
