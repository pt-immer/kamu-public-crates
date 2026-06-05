use kamu_logging::{
    Format, InitOptions, Sink,
    correlation::{DEFAULT_HEADER_CHAIN, extract_from_headers},
    info, init_with,
};
use worker::{Context, Env, Request, Response, Result, event};

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    init_logging(&env);

    let method = req.method().to_string();
    let path = req.path();
    let headers = req.headers();
    let correlation_id = extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, |headers, name| {
        headers.get(name).ok().flatten()
    });

    if let Some(correlation_id) = correlation_id.as_deref() {
        info!(%method, %path, %correlation_id, "handling Worker request");
    } else {
        info!(%method, %path, "handling Worker request");
    }

    Response::ok("hello from kamu-logging on Cloudflare Workers")
}

fn init_logging(env: &Env) {
    let mut options = InitOptions::default()
        .with_format(Format::Json)
        .with_sink(Sink::Stdout)
        .idempotent(true);

    if let Some(filter) = worker_log_filter(env) {
        options = options.with_default_filter(filter);
    }

    let _ = init_with(options);
}

fn worker_log_filter(env: &Env) -> Option<String> {
    env.var("RUST_LOG").ok().map(|value| value.to_string())
}
