use axum::{routing::get, Router};
use worker::*;

#[event(fetch)]
pub async fn main(_env: Env, _ctx: Context) -> Router<()> {
    let app: Router<()> = Router::new().route("/", get(|| async { "Hi!" }));
    app
}
