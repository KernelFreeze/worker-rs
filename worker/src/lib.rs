#![allow(clippy::new_without_default, clippy::or_fun_call)]

pub use axum::response::{IntoResponse, Response};
pub use tower::Service;
pub use worker_macros::event;
pub use worker_sys::{
    console_debug, console_error, console_log, console_warn, Context as JsContext,
    Request as JsRequest, Response as JsResponse,
};

#[doc(hidden)]
pub use wasm_bindgen;

#[doc(hidden)]
pub use wasm_bindgen_futures;

pub use crate::context::Context;
pub use crate::delay::Delay;
pub use crate::env::{Env, Secret, Var};
pub use crate::error::Error;
pub use crate::schedule::*;
pub use crate::streams::ByteStream;

mod context;
mod delay;
mod env;
mod error;
mod schedule;
mod streams;

pub type Result<T> = std::result::Result<T, error::Error>;
