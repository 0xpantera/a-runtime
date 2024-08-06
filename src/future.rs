use std::pin::Pin;
use crate::runtime::Waker;

pub trait Future {
    type Output;
    
    fn poll(self: Pin<&mut Self>, waker: &Waker) -> PollState<Self::Output>;
}

pub enum PollState<T> {
    Ready(T),
    NotReady,
}