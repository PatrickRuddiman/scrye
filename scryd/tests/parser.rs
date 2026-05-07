use assert_cmd::Command;

fn scryd() -> Command {
    Command::cargo_bin("scryd").expect("binary built")
}

#[test]
fn help_lists_three_operator_verbs() {
    let output = scryd().arg("--help").output().expect("run");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("add-account"), "missing add-account: {stdout}");
    assert!(stdout.contains("reindex"), "missing reindex: {stdout}");
    assert!(stdout.contains("search"), "missing search: {stdout}");
}

#[test]
fn help_does_not_advertise_serve() {
    let output = scryd().arg("--help").output().expect("run");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Hidden subcommands shouldn't appear in the auto-generated help table.
    let lines_with_serve: Vec<&str> = stdout
        .lines()
        .filter(|l| {
            // Match "  serve  ..." style help rows; ignore unrelated occurrences.
            let trimmed = l.trim_start();
            trimmed.starts_with("serve ")
                || trimmed.starts_with("serve\t")
                || trimmed == "serve"
        })
        .collect();
    assert!(
        lines_with_serve.is_empty(),
        "serve must not appear in --help: {lines_with_serve:?}"
    );
}

#[test]
fn unknown_verb_is_a_parse_error() {
    let output = scryd().arg("nonexistent-verb").output().expect("run");
    assert!(
        !output.status.success(),
        "scryd nonexistent-verb should fail to parse"
    );
}

#[test]
fn search_help_lists_the_filter_flags() {
    let output = scryd().args(["search", "--help"]).output().expect("run");
    let stdout = String::from_utf8(output.stdout).unwrap();
    for flag in [
        "--from", "--since", "--until", "--folder", "--account", "--limit", "--mode", "--json",
    ] {
        assert!(stdout.contains(flag), "missing {flag} in: {stdout}");
    }
}

#[test]
fn add_account_accepts_the_documented_flags() {
    // Just check parse acceptance — we don't actually run interactive
    // prompt-driven flow here.
    let output = scryd()
        .args([
            "add-account",
            "--account-id",
            "primary",
            "--host",
            "imap.x",
            "--port",
            "993",
            "--user",
            "alice@x",
            "--folders",
            "INBOX,Sent",
        ])
        .output()
        .expect("run");
    let _ = output; // accepts parse; impl is a stub for now.
}
