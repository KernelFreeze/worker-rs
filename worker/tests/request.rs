use axum::{body::BoxBody, routing::get, Router};
use worker::*;

#[event(fetch)]
pub async fn main(_env: Env, _ctx: Context) -> Router<BoxBody> {
    let app: Router<BoxBody> = Router::new().route("/", get(|| async { "Hi!" }));
    app
}
