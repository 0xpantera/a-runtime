mod future;
mod http;
mod runtime;
use crate::http::Http;
use future::{Future, PollState};
use runtime::Waker;
use std::{fmt::Write, marker::PhantomPinned, pin::Pin};

fn main() {
    let mut executor = runtime::init();
    executor.block_on(async_main());
}




// =================================
// We rewrite this:
// =================================
    
// coroutine fn async_main() {
//     println!("Program starting");
//     let txt = Http::get("/600/HelloAsyncAwait").wait;
//     println!("{txt}");
//     let txt = Http::get("/400/HelloAsyncAwait").wait;
//     println!("{txt}");

// }

// =================================
// Into this:
// =================================

fn async_main() -> impl Future<Output=String> {
    Coroutine0::new()
}
        
enum State0 {
    Start,
    Wait1(Pin<Box<dyn Future<Output = String>>>),
    Wait2(Pin<Box<dyn Future<Output = String>>>),
    Resolved,
}

#[derive(Default)]
struct Stack0 {
    counter: Option<usize>,
}

struct Coroutine0 {
    stack: Stack0,
    state: State0,
    _pin: PhantomPinned,
}

impl Coroutine0 {
    fn new() -> Self {
        Self { 
            state: State0::Start,
            stack: Stack0::default(),
            _pin: PhantomPinned,
        }
    }
}


impl Future for Coroutine0 {
    type Output = String;

    fn poll(self: Pin<&mut Self>, waker: &Waker) -> PollState<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        loop {
        match this.state {
                State0::Start => {
                    // Initialize stack (hoist variables)
                    this.stack.counter = Some(0);
                    // ---- Code you actually wrote ----
                    println!("Program starting");

                    // ---------------------------------
                    let fut1 = Box::pin( Http::get("/600/HelloAsyncAwait"));
                    this.state = State0::Wait1(fut1);
                    // Save stack (unnecessary here since just initialized)
                }

                State0::Wait1(ref mut f1) => {
                    match f1.as_mut().poll(waker) {
                        PollState::Ready(txt) => {
                            // Restore stack
                            let mut counter = this.stack.counter.take().unwrap();
                            // ---- Code you actually wrote ----
                            println!("{txt}");
                            counter += 1;
                            // ---------------------------------
                            let fut2 = Box::pin( Http::get("/400/HelloAsyncAwait"));
                            this.state = State0::Wait2(fut2);
                            // Save stack
                            this.stack.counter = Some(counter);
                        }
                        PollState::NotReady => break PollState::NotReady,
                    }
                }

                State0::Wait2(ref mut f2) => {
                    match f2.as_mut().poll(waker) {
                        PollState::Ready(txt) => {
                            // Restore stack
                            let mut counter = this.stack.counter.take().unwrap();
                            // ---- Code you actually wrote ----
                            println!("{txt}");
                            counter += 1;

                            println!("Received {} responses.", counter);
                            // ---------------------------------
                            this.state = State0::Resolved;
                            // Save stack (all variables set to `None` already)
                            break PollState::Ready(String::new());
                        }
                        PollState::NotReady => break PollState::NotReady,
                    }
                }

                State0::Resolved => panic!("Polled a resolved future")
            }
        }
    }
}
