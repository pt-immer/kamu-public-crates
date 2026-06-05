//! Actix Web example with correlation-enriched root spans.
//!
//! Run with `cargo run --example actix --features with-actix-web` then
//! `curl -H "X-Request-ID: req-abc123" http://127.0.0.1:8080/`. Logs will
//! include `correlation_id=req-abc123` on the root span.

use actix_web::{App, HttpResponse, HttpServer, Responder, get};
use kamu_logging::{get_actix_web_logger, info, init};

#[get("/")]
async fn index() -> impl Responder {
    info!("serving /");
    HttpResponse::Ok().body("hello\n")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init().expect("init logging");
    HttpServer::new(|| App::new().wrap(get_actix_web_logger()).service(index))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
