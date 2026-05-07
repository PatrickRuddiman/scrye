use scryd_imap::tombstone::{compute_tombstones, TOMBSTONE_SCAN_EVERY};

#[test]
fn returns_uids_present_locally_but_absent_from_server() {
    let local = [1, 2, 3, 4, 5];
    let server = [1, 3, 5];
    assert_eq!(compute_tombstones(&local, &server), vec![2, 4]);
}

#[test]
fn empty_diff_when_local_subset_of_server() {
    let local = [1, 2, 3];
    let server = [1, 2, 3, 4, 5];
    assert!(compute_tombstones(&local, &server).is_empty());
}

#[test]
fn all_tombstoned_when_server_set_empty() {
    let local = [1, 2, 3];
    let server: [u32; 0] = [];
    assert_eq!(compute_tombstones(&local, &server), vec![1, 2, 3]);
}

#[test]
fn empty_inputs_yield_empty_diff() {
    let local: [u32; 0] = [];
    let server: [u32; 0] = [];
    assert!(compute_tombstones(&local, &server).is_empty());
}

#[test]
fn duplicates_in_local_dedupe_in_diff() {
    let local = [1, 1, 2, 2, 3];
    let server = [3];
    assert_eq!(compute_tombstones(&local, &server), vec![1, 2]);
}

#[test]
fn output_is_sorted_ascending() {
    let local = [9, 5, 7, 3, 1];
    let server: [u32; 0] = [];
    assert_eq!(compute_tombstones(&local, &server), vec![1, 3, 5, 7, 9]);
}

#[test]
fn server_unique_uids_are_ignored() {
    // Server reports a UID we don't have locally — that's a "new message"
    // case for the fetcher to handle, not a tombstone.
    let local = [1, 2, 3];
    let server = [1, 2, 3, 4, 5];
    let diff = compute_tombstones(&local, &server);
    assert!(diff.is_empty());
}

#[test]
fn cadence_constant_is_10() {
    assert_eq!(TOMBSTONE_SCAN_EVERY, 10);
}
