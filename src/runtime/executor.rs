use crate::future::{Future, PollState};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::{Arc, Mutex},
    thread::{self, Thread},
};

type Task = Box<dyn Future<Output = String>>;

thread_local! {
    static CURRENT_EXEC: ExecutorCore = ExecutorCore::default();
}

#[derive(Default)]
struct ExecutorCore {
    // Holds all top-lvl futures associated with the executor
    // on this thread. `RefCell` needed to to mutate static variable.
    tasks: RefCell<HashMap<usize, Task>>,
    // Stores IDs of tasks that should be polled by executor. An `Arc`
    // (shared reference) to this `Vec` will be given to each `Waker`
    // that this executor creates. Since the `Waker` will be sent to a
    // different thread and signal that a task is ready by adding the 
    // tasks ID to `ready_queue`, it needs to be wrapped in `Arc<Mutex<_>>`
    ready_queue: Arc<Mutex<Vec<usize>>>,
    // Counter that gives out the next available ID. Should never hand out
    // the same ID twice for this executor instance. Since the executor 
    // instance will only be accessible on the same thread it was created,
    // `Cell` will suffice in giving us the needed internal mutability.
    next_id: Cell<usize>,
}

pub fn spawn<F>(future: F)
where 
    F: Future<Output = String> + 'static,
{
    CURRENT_EXEC.with(|e| {
        let id = e.next_id.get();
        e.tasks.borrow_mut().insert(id, Box::new(future));
        e.ready_queue.lock().map(|mut q| q.push(id)).unwrap();
        e.next_id.set(id + 1);
    });
}

pub struct Executor;

impl Executor {
    /// No initialization since everything is done lazily in `thread_local!`
    pub fn new() -> Self {
        Self {}
    }

    /// Pops off an ID that's ready from the back of the `ready_queue` `Vec`
    /// Since `Waker` pushes it's ID to the back of `ready_queue` and this pops
    /// from the back as well, this is essentially a LIFO queue.
    /// Using `VecDeque` from std lib would allow us to choose the order in which
    /// we remove items from the queue if we wish to change that behavior.
    fn pop_ready(&self) -> Option<usize> {
        CURRENT_EXEC.with(|q| q.ready_queue.lock().map(|mut q| q.pop()).unwrap())
    }

    /// Takes `id` of a top-lvl future, removes it from `tasks` and returns it if
    /// found. If the task returns `NotReady`, we need to remember to add it back
    /// to the collection again.
    fn get_future(&self, id: usize) -> Option<Task> {
        CURRENT_EXEC.with(|q| q.tasks.borrow_mut().remove(&id))
    }

    /// Creates a new `Waker` instance.
    fn get_waker(&self, id: usize) -> Waker {
        Waker {
            id,
            thread: thread::current(),
            ready_queue: CURRENT_EXEC.with(|q| q.ready_queue.clone()),
        }
    }

    /// Takes an `id` property and a `Task` property and inserts them into `tasks`
    fn insert_task(&self, id: usize, task: Task) {
        CURRENT_EXEC.with(|q| q.tasks.borrow_mut().insert(id, task));
    }

    /// Returns a count of how many tasks we have in the queue.
    fn task_count(&self) -> usize {
        CURRENT_EXEC.with(|q| q.tasks.borrow().len())
    }
}

#[derive(Clone)]
pub struct Waker {
    // Handle to the current Thread
    thread: Thread,
    // Identifies which task this Waker is associated with
    id: usize,
    // Reference to a `Vec<usize> that can be shared between 
    // threads. `usize` represents the ID of a task that's in
    // the ready queue. This object is shared with the executor
    // to push the task ID associated with the `Waker` onto that
    // queue when it's ready.
    ready_queue: Arc<Mutex<Vec<usize>>>,
}

impl Waker {
    /// When a `Waker::wake` is called, takes a lock on the `Mutex`
    /// that protects the ready queue shared with the executor.
    /// The `id` value for the task associated with this `Waker` is
    /// pushed onto the ready queue. After, `unpark` is called on the
    /// executor thread to wake it up. It will now find the task
    /// associated with this `Waker` in the ready queue and call `poll` on it.
    pub fn wake(&self) {
        self.ready_queue
            .lock()
            .map(|mut q| q.push(self.id))
            .unwrap();
        self.thread.unpark();
    }
}