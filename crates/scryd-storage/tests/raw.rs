use scryd_storage::raw::{open_raw, raw_path, write_raw};
use sha2::Digest;
use tempfile::TempDir;
use tokio::io::AsyncReadExt;

const APR_15_2026_MIDNIGHT_UTC: i64 = 1_776_211_200;

fn expected_filename(message_id: &str) -> String {
    format!("{}.eml", hex::encode(sha2::Sha256::digest(message_id.as_bytes())))
}

#[tokio::test]
async fn write_raw_lands_at_expected_path_with_correct_contents() {
    let dir = TempDir::new().unwrap();
    let bytes: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();

    let path = write_raw(
        dir.path(),
        "primary",
        "primary:abc@x",
        APR_15_2026_MIDNIGHT_UTC,
        &bytes,
    )
    .await
    .unwrap();

    let comps: Vec<String> = path
        .strip_prefix(dir.path())
        .unwrap()
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    assert_eq!(comps[0], "raw");
    assert_eq!(comps[1], "primary");
    assert_eq!(comps[2], "2026");
    assert_eq!(comps[3], "04");
    assert_eq!(comps[4], expected_filename("primary:abc@x"));

    let read_back = tokio::fs::read(&path).await.unwrap();
    assert_eq!(read_back, bytes);
}

#[cfg(unix)]
#[tokio::test]
async fn write_raw_sets_mode_0600() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let bytes = b"hello";
    let path = write_raw(dir.path(), "primary", "primary:perm@x", 0, bytes)
        .await
        .unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "file mode = {:o}", mode);
}

#[tokio::test]
async fn open_raw_streams_back_full_contents() {
    let dir = TempDir::new().unwrap();
    let bytes: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
    let path = write_raw(dir.path(), "primary", "primary:read@x", 0, &bytes)
        .await
        .unwrap();

    let mut f = open_raw(&path).await.unwrap();
    let mut buf = Vec::with_capacity(bytes.len());
    f.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, bytes);
}

#[tokio::test]
async fn raw_path_is_deterministic() {
    let dir = TempDir::new().unwrap();
    let p1 = raw_path(dir.path(), "primary", "primary:det@x", 1_700_000_000);
    let p2 = raw_path(dir.path(), "primary", "primary:det@x", 1_700_000_000);
    assert_eq!(p1, p2);
}

#[tokio::test]
async fn concurrent_writes_for_same_id_are_atomic() {
    let dir = TempDir::new().unwrap();
    let body_a: Vec<u8> = vec![1u8; 1024];
    let body_b: Vec<u8> = vec![2u8; 1024];

    let dir_a = dir.path().to_path_buf();
    let dir_b = dir.path().to_path_buf();
    let body_a_clone = body_a.clone();
    let body_b_clone = body_b.clone();

    let (a, b) = tokio::join!(
        async move {
            write_raw(&dir_a, "primary", "primary:race@x", 0, &body_a_clone).await
        },
        async move {
            write_raw(&dir_b, "primary", "primary:race@x", 0, &body_b_clone).await
        },
    );
    a.unwrap();
    b.unwrap();

    let path = raw_path(dir.path(), "primary", "primary:race@x", 0);
    let read_back = tokio::fs::read(&path).await.unwrap();
    assert!(
        read_back == body_a || read_back == body_b,
        "atomic rename guarantees the file is one of the two inputs, never a mix"
    );
}

#[tokio::test]
async fn write_raw_creates_missing_parents() {
    let dir = TempDir::new().unwrap();
    // Confirm the deeply nested parent dirs don't exist yet.
    assert!(!dir.path().join("raw").exists());
    write_raw(dir.path(), "primary", "primary:mkdir@x", 0, b"x")
        .await
        .unwrap();
    assert!(dir.path().join("raw/primary/1970/01").exists());
}
