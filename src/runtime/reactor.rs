use mio::{net::TcpStream, Events, Interest, Poll, Registry, Token};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    task::{Context, Waker},
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
    pub fn set_waker(&self, cx: &Context, id: usize) {
        let _ = self
            .wakers
            .lock()
            .map(|mut w| w.insert(id, cx.waker().clone()).is_none())
            .unwrap();
    }

    /// Removes the `Waker` from `wakers`. Then, derigisters the
    /// `TcpStream` from the `Poll` instance.
    pub fn deregister(&self, stream: &mut TcpStream, id: usize) {
        let _ = self.wakers.lock().map(|mut w| w.remove(&id).unwrap());
        self.registry.deregister(stream).unwrap();
    }

    /// Gets the current `next_id` and increments the counter atomically.
    /// We don't care about happens before/after relationships. We only care
    /// about not handing the same id twice, so `Ordering::Relaxed` is enough.
    pub fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

/// 1. Create an `events` collection.
/// 2. Loop indefinitely. Not ideal. No way to shut down event loop once started.
/// 3. Call `Poll::poll` with a timeout of `None`, meaning it will never time out
///    and block until it receives an event notification.
/// 4. When the call returns, loop through every event received.
/// 5. If an event is received it means something we registered interest in happened.
///    Get the `id` we passed when we first registered an interest in events on this
///    `TcpStream`.
/// 6. Try to get the associated `Waker` and call `Waker::wake` on it. Guard against
///    the fact that `Waker` may have been removed from the collection already, in which
///    case nothing is done.
fn event_loop(mut poll: Poll, wakers: Wakers) {
    let mut events = Events::with_capacity(100);
    loop {
        poll.poll(&mut events, None).unwrap();
        for e in events.iter() {
            let Token(id) = e.token();
            let wakers = wakers.lock().unwrap();

            if let Some(waker) = wakers.get(&id) {
                waker.wake_by_ref();
            }
        }
    }
}

pub fn start() {
    use thread::spawn;

    let wakers = Arc::new(Mutex::new(HashMap::new()));
    let poll = Poll::new().unwrap();
    let registry = poll.registry().try_clone().unwrap();
    let next_id = AtomicUsize::new(1);
    let reactor = Reactor {
        wakers: wakers.clone(),
        registry,
        next_id,
    };

    REACTOR.set(reactor).ok().expect("Reactor already running");
    spawn(move || event_loop(poll, wakers));
}