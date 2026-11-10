//! Minimal HTTP server exposing the metrics on `GET /metrics`.

use crate::options::Options;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use log::{debug, warn};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync>;
pub(crate) type RenderFn = fn(&Options) -> Result<String, BoxError>;

// Prometheus 3 rejects scrape targets that do not send a Content-Type.
const METRICS_CONTENT_TYPE: &str = "text/plain; version=0.0.4";

// Avoids a busy loop if accept() keeps failing (ie out of file descriptors).
const ACCEPT_ERROR_DELAY: Duration = Duration::from_millis(100);

fn response(status: StatusCode, body: String) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = status;
    response
}

fn handle_request(
    req: &Request<Incoming>,
    options: &Options,
    render: RenderFn,
) -> Response<Full<Bytes>> {
    if req.uri().path() != "/metrics" {
        return response(StatusCode::NOT_FOUND, String::new());
    }
    if req.method() != Method::GET {
        return response(StatusCode::METHOD_NOT_ALLOWED, String::new());
    }

    match render(options) {
        Ok(metrics) => {
            let mut response = response(StatusCode::OK, metrics);
            response.headers_mut().insert(
                CONTENT_TYPE,
                hyper::header::HeaderValue::from_static(METRICS_CONTENT_TYPE),
            );
            response
        }
        Err(err) => {
            warn!("internal server error == {:?}", err);
            response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
    }
}

/// Serves the output of `render` on `http://addr/metrics` until the process is stopped.
pub(crate) async fn serve(
    addr: SocketAddr,
    options: Arc<Options>,
    render: RenderFn,
) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (stream, remote_addr) = match listener.accept().await {
            Ok(connection) => connection,
            Err(err) => {
                warn!("failed to accept connection: {}", err);
                tokio::time::sleep(ACCEPT_ERROR_DELAY).await;
                continue;
            }
        };

        let options = options.clone();
        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let response = handle_request(&req, &options, render);
                async move { Ok::<_, Infallible>(response) }
            });

            // The timer enables hyper's header read timeout, so idle or
            // slow clients cannot keep connections open forever.
            if let Err(err) = http1::Builder::new()
                .timer(TokioTimer::new())
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                debug!("connection error from {}: {}", remote_addr, err);
            }
        });
    }
}
