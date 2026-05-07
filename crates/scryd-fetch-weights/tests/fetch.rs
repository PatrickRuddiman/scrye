#![cfg(unix)]

use std::convert::Infallible;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use assert_cmd::Command;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use sha2::Digest;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::Notify;

const PAYLOAD: &[u8] = b"some-fake-weights-payload-bytes-for-testing-only";

fn payload_sha256() -> String {
    hex::encode(sha2::Sha256::digest(PAYLOAD))
}

async fn spawn_fake_server(payload: &'static [u8]) -> (SocketAddr, Arc<Notify>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stop = Arc::new(Notify::new());
    let stop_clone = stop.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_clone.notified() => return,
                accept = listener.accept() => {
                    let Ok((tcp, _)) = accept else { continue };
                    let io = TokioIo::new(tcp);
                    tokio::spawn(async move {
                        let _ = http1::Builder::new()
                            .serve_connection(
                                io,
                                service_fn(move |_req: Request<hyper::body::Incoming>| async move {
                                    let body = Full::new(Bytes::from_static(payload));
                                    Ok::<_, Infallible>(Response::new(body))
                                }),
                            )
                            .await;
                    });
                }
            }
        }
    });

    (addr, stop)
}

fn helper() -> Command {
    Command::cargo_bin("scryd-fetch-weights").expect("binary built")
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

#[test]
fn downloads_writes_at_mode_0600_with_correct_hash() {
    block_on(async {
        let (addr, _stop) = spawn_fake_server(PAYLOAD).await;
        let url = format!("http://{addr}/xtr-weights.gguf");
        let dir = TempDir::new().unwrap();
        let target = dir.path().to_path_buf();
        let sha = payload_sha256();

        let output = tokio::task::spawn_blocking(move || {
            helper()
                .env("SCRYD_FETCH_WEIGHTS_ALLOW_ROOT", "1")
                .args([
                    "--target",
                    target.to_str().unwrap(),
                    "--url",
                    &url,
                    "--sha256",
                    &sha,
                ])
                .output()
                .expect("run")
        })
        .await
        .unwrap();

        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let weights: PathBuf = dir.path().join("xtr-weights.gguf");
        assert!(weights.exists(), "weights file should exist");
        let mode = std::fs::metadata(&weights).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "mode = {:o}", mode);
        let actual = std::fs::read(&weights).unwrap();
        assert_eq!(actual, PAYLOAD);
    });
}

#[test]
fn second_invocation_with_correct_hash_is_a_noop() {
    block_on(async {
        let (addr, _stop) = spawn_fake_server(PAYLOAD).await;
        let url = format!("http://{addr}/xtr-weights.gguf");
        let dir = TempDir::new().unwrap();
        let target = dir.path().to_path_buf();
        let sha = payload_sha256();

        // First call: downloads.
        let target_for_first = target.clone();
        let url_for_first = url.clone();
        let sha_for_first = sha.clone();
        let _ = tokio::task::spawn_blocking(move || {
            helper()
                .env("SCRYD_FETCH_WEIGHTS_ALLOW_ROOT", "1")
                .args([
                    "--target",
                    target_for_first.to_str().unwrap(),
                    "--url",
                    &url_for_first,
                    "--sha256",
                    &sha_for_first,
                ])
                .output()
                .expect("run")
        })
        .await
        .unwrap();
        let mtime_first = std::fs::metadata(dir.path().join("xtr-weights.gguf"))
            .unwrap()
            .modified()
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Second call with the same hash: must NOT re-write the file.
        let output = tokio::task::spawn_blocking(move || {
            helper()
                .env("SCRYD_FETCH_WEIGHTS_ALLOW_ROOT", "1")
                .args([
                    "--target",
                    target.to_str().unwrap(),
                    "--url",
                    &url,
                    "--sha256",
                    &sha,
                ])
                .output()
                .expect("run")
        })
        .await
        .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("weights ok"), "{stdout}");
        let mtime_second = std::fs::metadata(dir.path().join("xtr-weights.gguf"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            mtime_first, mtime_second,
            "second call should be a no-op (no file rewrite)"
        );
    });
}

#[test]
fn corrupt_existing_file_is_redownloaded() {
    block_on(async {
        let (addr, _stop) = spawn_fake_server(PAYLOAD).await;
        let url = format!("http://{addr}/xtr-weights.gguf");
        let dir = TempDir::new().unwrap();
        let target = dir.path().to_path_buf();
        let sha = payload_sha256();

        // Pre-write a corrupt file at the target path.
        let weights = target.join("xtr-weights.gguf");
        std::fs::write(&weights, b"wrong bytes").unwrap();

        let output = tokio::task::spawn_blocking(move || {
            helper()
                .env("SCRYD_FETCH_WEIGHTS_ALLOW_ROOT", "1")
                .args([
                    "--target",
                    target.to_str().unwrap(),
                    "--url",
                    &url,
                    "--sha256",
                    &sha,
                ])
                .output()
                .expect("run")
        })
        .await
        .unwrap();

        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = std::fs::read(&weights).unwrap();
        assert_eq!(actual, PAYLOAD, "corrupt file should be re-downloaded");
    });
}

#[test]
fn hash_mismatch_after_download_fails_loudly() {
    block_on(async {
        let (addr, _stop) = spawn_fake_server(PAYLOAD).await;
        let url = format!("http://{addr}/xtr-weights.gguf");
        let dir = TempDir::new().unwrap();
        let target = dir.path().to_path_buf();
        let bogus_sha = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

        let output = tokio::task::spawn_blocking(move || {
            helper()
                .env("SCRYD_FETCH_WEIGHTS_ALLOW_ROOT", "1")
                .args([
                    "--target",
                    target.to_str().unwrap(),
                    "--url",
                    &url,
                    "--sha256",
                    bogus_sha,
                ])
                .output()
                .expect("run")
        })
        .await
        .unwrap();

        assert!(
            !output.status.success(),
            "expected non-zero exit on hash mismatch"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("hash") && stderr.contains("aborting"),
            "expected hash-aborting error: {stderr}"
        );
        let weights: PathBuf = dir.path().join("xtr-weights.gguf");
        assert!(
            !weights.exists(),
            "tmp file should not be left behind on hash mismatch"
        );
    });
}
