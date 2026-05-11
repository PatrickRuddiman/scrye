//! Drive axum over a Unix-domain-socket listener with cooperative
//! shutdown. Optional peercred backstop runs at accept time when
//! `require_peer_uid` is on.

use std::future::Future;

use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use tokio::net::UnixListener;

use crate::peercred::check_stream_peer;
use crate::ApiError;

/// Accept connections on `listener`, optionally peercred-check each
/// one, and serve them through `router`. `shutdown` is awaited; once
/// it resolves, the loop stops accepting and existing connections
/// drain.
///
/// `require_peer_uid` controls the kernel-level access gate. The
/// v0.3.1 default is `false` (open service shape — anyone who can
/// reach the socket can call the api; the consumer's higher-layer
/// api is the auth boundary). Set `true` (via `[server]
/// require_peer_uid = true` in config) for same-uid-only enforcement.
pub async fn serve(
    listener: UnixListener,
    router: Router,
    require_peer_uid: bool,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), ApiError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    let shutdown_signal = async move {
        shutdown.await;
        let _ = tx.send(());
    };
    tokio::pin!(shutdown_signal);

    let svc = router.into_make_service();

    loop {
        tokio::select! {
            _ = &mut shutdown_signal => return Ok(()),
            _ = rx.recv() => return Ok(()),
            accept = listener.accept() => {
                let (stream, _addr) = match accept {
                    Ok(a) => a,
                    Err(e) => {
                        return Err(ApiError::Bind(e));
                    }
                };
                if check_stream_peer(&stream, require_peer_uid).is_err() {
                    // Rejection log was already emitted (only when
                    // require_peer_uid was on; off path returns Ok).
                    continue;
                }
                let mut svc = svc.clone();
                tokio::spawn(async move {
                    use tower::Service as _;
                    let tower_svc = match svc.call(()).await {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let io = TokioIo::new(stream);
                    let hyper_svc = hyper::service::service_fn(
                        move |req: hyper::Request<hyper::body::Incoming>| {
                            let mut tower_svc = tower_svc.clone();
                            async move {
                                let req = req.map(axum::body::Body::new);
                                tower::Service::<axum::http::Request<axum::body::Body>>::call(
                                    &mut tower_svc,
                                    req,
                                )
                                .await
                            }
                        },
                    );
                    let _ = Builder::new(TokioExecutor::new())
                        .serve_connection(io, hyper_svc)
                        .await;
                });
            }
        }
    }
}

/// Convenience: a graceful-shutdown future that resolves when SIGTERM /
/// SIGINT arrives. Tests use a different signal.
pub async fn graceful_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = sigint.recv() => {}
    }
}

