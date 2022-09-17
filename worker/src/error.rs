use thiserror::Error;
use wasm_bindgen::{JsCast, JsValue};

/// All possible Error variants that might be encountered while working with a Worker.
#[derive(Error, Debug)]
pub enum Error {
    #[error("content-type mismatch")]
    BadEncoding,

    #[error("failed to encode data to json")]
    Json,

    #[error("{0}")]
    JsError(String),

    #[error("unrecognized JavaScript object")]
    Internal,

    #[error("error status codes must be in the 400-599 range")]
    StatusCode,

    #[error("redirect status codes must be in the 300-399 range")]
    Redirect,

    #[error("no binding found for `{0}`")]
    BindingError(String),

    #[error("failed to insert route")]
    RouteInsertError(#[from] matchit::InsertError),

    #[error("route has no corresponding shared data")]
    RouteNoDataError,

    #[error("{0}")]
    Message(String),

    #[error("{0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Cloudflare K/V error")]
    KvError,

    #[error("failed to read body from request")]
    ReadBody,

    #[error("{0}")]
    Http(#[from] http::Error),

    #[error("{0}")]
    HeaderValue(#[from] http::header::InvalidHeaderValue),

    #[error("{0}")]
    HeaderName(#[from] http::header::InvalidHeaderName),

    #[error("{0}")]
    HeaderValueContent(#[from] http::header::ToStrError),

    #[error("{0}")]
    Method(#[from] http::method::InvalidMethod),

    #[error("{0}")]
    BoxedError(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl From<JsValue> for Error {
    fn from(v: JsValue) -> Self {
        match v
            .as_string()
            .or_else(|| v.dyn_ref::<js_sys::Error>().map(|e| e.to_string().into()))
        {
            Some(s) => Self::JsError(s),
            None => Self::Internal,
        }
    }
}
