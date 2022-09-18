use axum::body::{Body, Bytes};
use axum::response::IntoResponse;
use http::{header::HeaderName, HeaderValue, Method, Request};
use js_sys::{Iterator, Uint8Array};
use std::pin::Pin;
use std::str::FromStr;
use wasm_bindgen_futures::JsFuture;
use worker_sys::Request as JsRequest;
use worker_sys::Response;

use crate::{Error, Result};

pub async fn convert_request(edge_request: JsRequest) -> Result<Request<Body>> {
    let method = Method::from_str(&edge_request.method())?;
    let uri = edge_request.url();
    let body: Bytes = JsFuture::from(edge_request.array_buffer()?)
        .await
        .map(|val| Uint8Array::new(&val).to_vec())?
        .into();
    let body = Body::from(body);

    let mut request = Request::builder().method(method).uri(uri).body(body)?;

    if let Ok(entries) = edge_request.headers().entries() {
        let headers = request.headers_mut();
        for entry in entries.into_iter().flatten() {
            let iterator = Iterator::from(entry);
            let key = iterator.next()?.as_string();
            let value = iterator.next()?.as_string();
            if let Some(key) = key {
                if let Some(value) = value {
                    headers.insert(HeaderName::try_from(key)?, HeaderValue::try_from(value)?);
                }
            }
        }
    }

    Ok(request)
}

pub fn convert_response(response: impl IntoResponse) -> Result<Response> {
    use axum::body::HttpBody;
    use futures_util::TryStreamExt;
    use wasm_bindgen::JsCast;

    let mut response = response.into_response();

    let headers = worker_sys::Headers::new()?;
    for (key, value) in response.headers() {
        headers.append(key.as_str(), value.to_str()?)?;
    }

    let mut init = worker_sys::ResponseInit::new();
    init.status(response.status().as_u16());
    init.headers(&headers);

    let stream =
        futures_util::stream::poll_fn(move |ctx| Pin::new(response.body_mut()).poll_data(ctx));

    let js_stream = stream
        .map_ok(|chunk| {
            let array = Uint8Array::new_with_length(chunk.len() as u32);
            array.copy_from(&chunk);
            wasm_bindgen::JsValue::from(array)
        })
        .map_err(|e| wasm_bindgen::JsValue::from(e.to_string()));

    let stream = wasm_streams::ReadableStream::from_stream(js_stream);
    let stream = stream.into_raw().dyn_into().map_err(|_| Error::ReadBody)?;

    let response = Response::new_with_opt_stream_and_init(Some(stream), &init)?;
    Ok(response)
}
