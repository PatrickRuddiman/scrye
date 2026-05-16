//! Regression: workspace feature unification compiles in both
//! `rustls/aws-lc-rs` (via this crate + tokio-rustls defaults) and
//! `rustls/ring` (via scryd-fetch-weights → reqwest's rustls-tls
//! feature path). rustls 0.23 refuses to auto-select between
//! compiled-in providers and panics on the first
//! `ClientConfig::builder()` call.
//!
//! `scryd::main` installs the `aws-lc-rs` provider at process
//! startup; this test installs it in the test process and then
//! exercises the same builder API to prove that, once a provider
//! is explicitly chosen, the unified-features build graph can
//! construct a TLS client config without panicking. Drop the
//! install line below and this test reproduces the v0.3.1 crash.

use rustls::{ClientConfig, RootCertStore};

#[test]
fn rustls_client_config_builds_under_unified_feature_graph() {
    // `install_default` returns Err if another test in the same
    // process already installed a provider — that's fine, we only
    // need *some* provider installed before the builder runs.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let roots = RootCertStore::empty();
    let _cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
}
