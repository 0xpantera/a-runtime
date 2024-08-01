use crate::future::{Future, PollState};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::{Arc, Mutex},
    thread::{self, Thread},
};

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