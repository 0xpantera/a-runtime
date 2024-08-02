use crate::runtime::Waker;
use mio::{net::TcpStream, Events, Interest, Poll, Registry, Token};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
};

type Wakers = Arc<Mutex<HashMap<usize, Waker>>>;

// Ensure that there can only be a single instance of this
// specific `Reactor` running in our program.
static REACTOR: OnceLock<Reactor> = OnceLock::new();

pub fn reactor() -> &'static Reactor {
    REACTOR.get().expect("Called outside a runtime context")
}

pub struct Reactor {
    /// Hashmap of Waker objects identified by usize
    wakers: Wakers,
    /// Registry instance to interact with the event queue in mio
    registry: Registry,
    /// Tracks which event occured & which `Waker` should be woken
    next_id: AtomicUsize,
}

impl Reactor {

    /// Wrapper around `Registry::register`. `id` property is passed to
    /// identify which event has occured when a notification is received later.
    pub fn register(
        &self,
        stream: &mut TcpStream,
        interest: Interest,
        id: usize
    )
    {
        self.registry.register(stream, Token(id), interest).unwrap();
    }

    /// Adds a `Waker` to the `HashMap` using the provided `id` property
    /// as a key to identify it. If a `Waker` already exists, it's replaced
    /// and the old one is dropped. The most recent `Waker` should always be
    /// the one stored so that this fn can be called multiple times, eventhough
    /// there is already a `Waker` associated with the `TcpStream`.
    pub fn set_waker(&self, waker: &Waker, id: usize) {
        let _ = self
            .wakers
            .lock()
            .map(|mut w| w.insert(id, waker.clone()).is_none())
            .unwrap();
    }

    /// Removes the `Waker` from `wakers`. Then, derigisters the
    /// `TcpStream` from the `Poll` instance.
    pub fn deregister(&self, stream: &mut TcpStream, id: usize) {
        self.wakers.lock().map(|mut w| w.remove(&id).unwrap());
        self.registry.deregister(stream).unwrap();
    }

    /// Gets the current `next_id` and increments the counter atomically.
    /// We don't care about happens before/after relationships. We only care
    /// about not handing the same id twice, so `Ordering::Relaxed` is enough.
    pub fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}