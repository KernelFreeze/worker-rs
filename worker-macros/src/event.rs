use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, punctuated::Punctuated, token::Comma, Ident, ItemFn};

pub fn expand_macro(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs: Punctuated<Ident, Comma> =
        parse_macro_input!(attr with Punctuated::parse_terminated);

    enum HandlerType {
        Fetch,
        Scheduled,
        Start,
    }
    use HandlerType::*;

    let mut handler_type = None;
    let mut respond_with_errors = false;

    for attr in attrs {
        match attr.to_string().as_str() {
            "fetch" => handler_type = Some(Fetch),
            "scheduled" => handler_type = Some(Scheduled),
            "start" => handler_type = Some(Start),
            "respond_with_errors" => {
                respond_with_errors = true;
            }
            _ => panic!("Invalid attribute: {}", attr),
        }
    }
    let handler_type = handler_type.expect(
        "must have either 'fetch', 'scheduled', or 'start' attribute, e.g. #[event(fetch)]",
    );

    // create new var using syn item of the attributed fn
    let mut input_fn = parse_macro_input!(item as ItemFn);

    match handler_type {
        Fetch => {
            // save original fn name for re-use in the wrapper fn
            let input_fn_ident = Ident::new(
                &(input_fn.sig.ident.to_string() + "_fetch_glue"),
                input_fn.sig.ident.span(),
            );
            let wrapper_fn_ident = Ident::new("fetch", input_fn.sig.ident.span());
            // rename the original attributed fn
            input_fn.sig.ident = input_fn_ident.clone();

            let error_handling = if respond_with_errors {
                quote! {
                    ::worker::Response::error(e.to_string(), 500).unwrap().into()
                }
            } else {
                quote! { panic!("{}", e) }
            };

            let convert_response_fn = quote! {
                fn convert_response(
                    response: impl ::axum::response::IntoResponse,
                ) -> Result<::worker_sys::Response, ::worker::Error> {
                    use futures_util::TryStreamExt;
                    use axum::body::HttpBody;
                    use wasm_bindgen::JsCast;

                    let mut response = response.into_response();

                    let headers = worker_sys::Headers::new()?;
                    for (key, value) in response.headers() {
                        headers.append(key.as_str(), value.to_str()?)?;
                    }

                    let mut init = worker_sys::ResponseInit::new();
                    init.status(response.status().as_u16());
                    init.headers(&headers);

                    let stream = futures_util::stream::poll_fn(move |ctx| std::pin::Pin::new(response.body_mut()).poll_data(ctx));

                    let js_stream = stream
                        .map_ok(|chunk| {
                            let array = js_sys::Uint8Array::new_with_length(chunk.len() as u32);
                            array.copy_from(&chunk);
                            wasm_bindgen::JsValue::from(array)
                        })
                        .map_err(|e| wasm_bindgen::JsValue::from(e.to_string()));

                    let stream = wasm_streams::ReadableStream::from_stream(js_stream);
                    let stream = stream
                        .into_raw()
                        .dyn_into()
                        .map_err(|_| worker::Error::ReadBody)?;

                    let response = worker_sys::Response::new_with_opt_stream_and_init(Some(stream), &init)?;
                    Ok(response)
                }
            };

            let convert_request_fn = quote! {
                async fn convert_request(
                    edge_request: worker_sys::Request,
                ) -> Result<http::Request<axum::body::Body>, worker::Error> {
                    use axum::body::Bytes;
                    use http::{header::HeaderName, HeaderValue, Method, Request};
                    use js_sys::Iterator;
                    use std::str::FromStr;
                    use wasm_bindgen_futures::JsFuture;
                    let method = Method::from_str(&edge_request.method())?;
                    let uri = edge_request.url();
                    let body: Bytes = JsFuture::from(edge_request.array_buffer()?)
                        .await
                        .map(|val| js_sys::Uint8Array::new(&val).to_vec())?
                        .into();
                    let body = axum::body::Body::from(body);
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
            };

            let service_fn = quote! {
                async fn service(
                    req: ::worker::JsRequest,
                    env: ::worker::Env,
                    ctx: ::worker::JsContext
                ) -> Result<::worker_sys::Response, ::worker::Error> {
                    use ::worker::Service;

                    let ctx = ::worker::Context::new(ctx);
                    let req = convert_request(req).await?;

                    // get the impl IntoResponse by calling the original fn
                    let mut service = #input_fn_ident(env, ctx).await;
                    std::future::poll_fn(|cx| service.poll_ready(cx)).await.unwrap();
                    convert_response(service.call(req).await.unwrap())
                }
            };

            // create a new "main" function that takes the worker_sys::Request, and calls the
            // original attributed function, passing in a converted worker::Request
            let wrapper_fn = quote! {
                pub async fn #wrapper_fn_ident(
                    req: ::worker::JsRequest,
                    env: ::worker::Env,
                    ctx: ::worker::JsContext
                ) -> ::worker::JsResponse {
                    match service(req, env, ctx).await {
                        Ok(res) => res,
                        Err(e) => {
                            ::worker::console_log!("{}", &e);
                            #error_handling
                        }
                    }
                }
            };

            let wasm_bindgen_code =
                wasm_bindgen_macro_support::expand(TokenStream::new().into(), wrapper_fn)
                    .expect("wasm_bindgen macro failed to expand");

            let output = quote! {
                #input_fn

                mod _worker_fetch {
                    use ::worker::{wasm_bindgen, wasm_bindgen_futures};
                    use super::#input_fn_ident;
                    #wasm_bindgen_code
                    #convert_request_fn
                    #convert_response_fn
                    #service_fn
                }
            };

            TokenStream::from(output)
        }
        Scheduled => {
            // save original fn name for re-use in the wrapper fn
            let input_fn_ident = Ident::new(
                &(input_fn.sig.ident.to_string() + "_scheduled_glue"),
                input_fn.sig.ident.span(),
            );
            let wrapper_fn_ident = Ident::new("scheduled", input_fn.sig.ident.span());
            // rename the original attributed fn
            input_fn.sig.ident = input_fn_ident.clone();

            let wrapper_fn = quote! {
                pub async fn #wrapper_fn_ident(event: ::worker::worker_sys::ScheduledEvent, env: ::worker::Env, ctx: ::worker::worker_sys::ScheduleContext) {
                    // call the original fn
                    #input_fn_ident(::worker::ScheduledEvent::from(event), env, ::worker::ScheduleContext::from(ctx)).await
                }
            };
            let wasm_bindgen_code =
                wasm_bindgen_macro_support::expand(TokenStream::new().into(), wrapper_fn)
                    .expect("wasm_bindgen macro failed to expand");

            let output = quote! {
                #input_fn

                mod _worker_scheduled {
                    use ::worker::{wasm_bindgen, wasm_bindgen_futures};
                    use super::#input_fn_ident;
                    #wasm_bindgen_code
                }
            };

            TokenStream::from(output)
        }
        Start => {
            // save original fn name for re-use in the wrapper fn
            let input_fn_ident = Ident::new(
                &(input_fn.sig.ident.to_string() + "_start_glue"),
                input_fn.sig.ident.span(),
            );
            let wrapper_fn_ident = Ident::new("start", input_fn.sig.ident.span());
            // rename the original attributed fn
            input_fn.sig.ident = input_fn_ident.clone();

            let wrapper_fn = quote! {
                pub fn #wrapper_fn_ident() {
                    // call the original fn
                    #input_fn_ident()
                }
            };
            let wasm_bindgen_code =
                wasm_bindgen_macro_support::expand(quote! { start }, wrapper_fn)
                    .expect("wasm_bindgen macro failed to expand");

            let output = quote! {
                #input_fn

                mod _worker_start {
                    use ::worker::{wasm_bindgen, wasm_bindgen_futures};
                    use super::#input_fn_ident;
                    #wasm_bindgen_code
                }
            };

            TokenStream::from(output)
        }
    }
}
