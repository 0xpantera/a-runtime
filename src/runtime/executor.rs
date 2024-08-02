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

    /// Entry point to `Executor`:
    /// 1. Spawns the future received.
    /// 2. Loop as long as our asynchronous programs is running.
    /// 3. On every iteration, create an inner loop that runs as long as 
    ///    there are tasks in `ready_queue`.
    /// 4. If there is a task in `ready_queue`, take ownership of the `Future` 
    ///    by removing from collection. Guard against false wakeups by continuing
    ///    if there is no future in it anymore.
    /// 5. Create a `Waker` instance to pass into `Future::poll()`. This `Waker`
    ///    instance now holds the `id` property that IDs this specific `Future` and
    ///    a handle to the thread it's currently running on.
    /// 6. Call `Future::poll`. If `NotReady` insert back into `tasks`. If `Ready` it
    ///    continues to the next item in the `ready_queue`. The `Future` will be dropped
    ///    before next iteration of `while let` loop because it took ownership.
    /// 7. After polling all task in `ready_queue` get `tasks` count to see how many left.
    /// 8. If there are tasks left call `thread::park()`. This will yield control to the 
    ///    OS scheduler, and `Executor` does nothing until it's woken up again.
    /// 9. If there are no tasks left the program is done and exit the main loop.
    pub fn block_on<F>(&mut self, future: F)
    where
        F: Future<Output = String> + 'static,
    {
        spawn(future);

        loop {
            while let Some(id) = self.pop_ready() {
                let mut future = match self.get_future(id) {
                    Some(f) => f,
                    // guard against false wakeups
                    None => continue,
                };
                let waker = self.get_waker(id);

                match future.poll(&waker) {
                    PollState::NotReady => self.insert_task(id, future),
                    PollState::Ready(_) => continue,
                }
            }

            let task_count = self.task_count();
            let name = thread::current().name().unwrap_or_default().to_string();

            if task_count > 0 {
                println!("{name}: {task_count} pending tasks. Sleep until notified.");
                thread::park();
            } else {
                println!("{name}: All tasks are finished");
                break;
            }
        }
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
    /// When `Waker::wake` is called, takes a lock on the `Mutex`
    /// that protects the `ready_queue` shared with the `Executor`.
    /// The `id` value for the task associated with this `Waker` is
    /// pushed onto the ready queue. After, `unpark` is called on the
    /// `Executor` thread to wake it up. It will now find the task
    /// associated with this `Waker` in the ready queue and call `poll` on it.
    pub fn wake(&self) {
        self.ready_queue
            .lock()
            .map(|mut q| q.push(self.id))
            .unwrap();
        self.thread.unpark();
    }
}