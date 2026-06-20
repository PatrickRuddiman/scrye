//! End-to-end Streamable-HTTP round trip: serve the MCP surface on an ephemeral
//! loopback port and drive it with a real `rmcp` Streamable-HTTP client. Proves
//! the wire transport works on Windows (no `#![cfg(unix)]`), advertises all six
//! read tools, and that the `USER_EMAIL` scope holds over the network.

mod common;

use common::*;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt;

#[tokio::test]
async fn streamable_http_roundtrip_enforces_scope() {
    let seed = seed().await;
    let state = seed.state();

    // Bind an OS-assigned loopback port, then serve on the bound listener.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = scryd_mcp::serve_listener(state, listener, std::future::pending::<()>()).await;
    });

    // Real rmcp Streamable-HTTP client over reqwest.
    let transport = StreamableHttpClientTransport::from_uri(format!("http://{addr}/mcp"));
    let client = ().serve(transport).await.unwrap();

    // 1) All six read tools are advertised, by name.
    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    names.sort_unstable();
    let mut expected = vec![
        "get_message",
        "get_raw_message",
        "get_thread",
        "list_accounts",
        "search",
        "status",
    ];
    expected.sort_unstable();
    assert_eq!(names, expected);

    // 2) `status` succeeds and reports healthy (`ok: true`).
    let status = client
        .call_tool(CallToolRequestParams::new("status"))
        .await
        .unwrap();
    assert!(status.is_error != Some(true));
    let text = status
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .expect("status returns text content");
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["ok"], serde_json::Value::Bool(true));
    // The issue-#20 drainer + daemon health blocks must survive the wire trip.
    assert!(parsed["drainer"].is_object());
    assert!(parsed["daemon"].is_object());
    assert_eq!(parsed["daemon"]["in_crash_loop"], serde_json::Value::Bool(false));

    // 3) A foreign message id must not succeed over the wire — same response a
    //    missing id would get (no existence leak).
    let mut args = serde_json::Map::new();
    args.insert(
        "id".to_string(),
        serde_json::Value::String(FOREIGN_MSG.to_string()),
    );
    let res = client
        .call_tool(CallToolRequestParams::new("get_message").with_arguments(args))
        .await;
    let denied = match res {
        Err(_) => true,
        Ok(result) => result.is_error == Some(true),
    };
    assert!(denied, "foreign get_message must not succeed");

    let _ = client.cancel().await;
    server.abort();
}
