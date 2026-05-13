#![cfg(unix)]

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
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

const REQUIRED_FILES: &[&str] = &[
    "tokenizer.json",
    "config.json",
    "xtr-ov-int4.xml",
    "xtr-ov-int4.bin",
];

/// Build a small tarball matching the production layout (single top-
/// level dir containing the four files witchcraft expects). Returns
/// the tarball bytes and their SHA-256.
fn build_fixture_tarball() -> (Vec<u8>, String) {
    let staging = TempDir::new().unwrap();
    let bundle_dir = staging.path().join("xtr-int4-test");
    std::fs::create_dir(&bundle_dir).unwrap();
    for (i, name) in REQUIRED_FILES.iter().enumerate() {
        // Distinct contents so a sloppy extract that overwrites with
        // the wrong file shows up as a mismatch.
        std::fs::write(bundle_dir.join(name), format!("fixture-{i}\n")).unwrap();
    }
    let tarball = staging.path().join("bundle.tar.gz");
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(staging.path())
        .arg("xtr-int4-test")
        .status()
        .expect("tar -czf available on PATH");
    assert!(status.success(), "fixture tar -czf failed");
    let bytes = std::fs::read(&tarball).unwrap();
    let sha = hex::encode(sha2::Sha256::digest(&bytes));
    // Leak the bytes so the static-payload server signature stays
    // happy across the suite (tests are short-lived).
    (bytes, sha)
}

async fn spawn_static_server(payload: &'static [u8]) -> (SocketAddr, Arc<Notify>) {
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

fn all_present(dir: &Path) -> bool {
    REQUIRED_FILES.iter().all(|f| dir.join(f).exists())
}

#[test]
fn downloads_and_extracts_with_correct_hash() {
    block_on(async {
        let (bytes, sha) = build_fixture_tarball();
        let payload: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let (addr, _stop) = spawn_static_server(payload).await;
        let url = format!("http://{addr}/xtr-int4.tar.gz");
        let dir = TempDir::new().unwrap();
        let target = dir.path().to_path_buf();

        let output = tokio::task::spawn_blocking(move || {
            helper()
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
        assert!(all_present(dir.path()), "all four files should be extracted");
        // The tmp tarball should have been cleaned up.
        assert!(
            !dir.path().join(".fetch-tmp.tar.gz").exists(),
            "tmp tarball should be removed after extract"
        );
    });
}

#[test]
fn second_invocation_with_files_present_is_a_noop() {
    block_on(async {
        let (bytes, sha) = build_fixture_tarball();
        let payload: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let (addr, _stop) = spawn_static_server(payload).await;
        let url = format!("http://{addr}/xtr-int4.tar.gz");
        let dir = TempDir::new().unwrap();
        let target = dir.path().to_path_buf();

        // First call: downloads + extracts.
        let t1 = target.clone();
        let u1 = url.clone();
        let s1 = sha.clone();
        let _ = tokio::task::spawn_blocking(move || {
            helper()
                .args([
                    "--target",
                    t1.to_str().unwrap(),
                    "--url",
                    &u1,
                    "--sha256",
                    &s1,
                ])
                .output()
                .expect("run")
        })
        .await
        .unwrap();
        assert!(all_present(dir.path()));
        let mtime_first = std::fs::metadata(dir.path().join("tokenizer.json"))
            .unwrap()
            .modified()
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Second call: must NOT re-download or re-extract.
        let output = tokio::task::spawn_blocking(move || {
            helper()
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
        assert!(stdout.contains("assets ok"), "{stdout}");
        let mtime_second = std::fs::metadata(dir.path().join("tokenizer.json"))
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
fn hash_mismatch_fails_loudly_and_leaves_no_artifacts() {
    block_on(async {
        let (bytes, _sha) = build_fixture_tarball();
        let payload: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let (addr, _stop) = spawn_static_server(payload).await;
        let url = format!("http://{addr}/xtr-int4.tar.gz");
        let dir = TempDir::new().unwrap();
        let target = dir.path().to_path_buf();
        let bogus_sha = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

        let output = tokio::task::spawn_blocking(move || {
            helper()
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
        // Neither the tmp tarball nor any extracted file should remain.
        assert!(!dir.path().join(".fetch-tmp.tar.gz").exists());
        for f in REQUIRED_FILES {
            assert!(
                !dir.path().join(f).exists(),
                "no extracted file should be left behind on hash failure: {f}"
            );
        }
        let _: PathBuf;
    });
}
