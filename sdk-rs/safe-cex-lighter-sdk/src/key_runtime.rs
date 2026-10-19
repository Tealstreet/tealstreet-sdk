//! Future capability shared by the shipped SDK HTTP client and read bodies.
use std::{future::Future, pin::Pin};

pub type KeyFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>>>>;
