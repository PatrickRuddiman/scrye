//! Streamable-HTTP transport. Mounts the [`McpServer`] at `/mcp` on a loopback
//! TCP listener and serves it with graceful shutdown.

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};

use crate::server::McpServer;
use crate::state::McpState;

/// Bind a loopback `TcpListener` to `bind` and serve the MCP surface until
/// `shutdown` resolves.
pub async fn serve(
    state: McpState,
    bind: std::net::SocketAddr,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    serve_listener(state, listener, shutdown).await
}

/// Serve the MCP surface on an already-bound listener. Split out from [`serve`]
/// so callers (notably the round-trip test) can bind `127.0.0.1:0`, read the
/// OS-assigned port, and then start serving.
pub async fn serve_listener(
    state: McpState,
    listener: tokio::net::TcpListener,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let service = StreamableHttpService::new(
        move || Ok(McpServer::new(state.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
}
