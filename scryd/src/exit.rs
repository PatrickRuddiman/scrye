//! Closed-set process exit codes per cli slice §3 Decision 11.

// Tasks 21–23 fill in the verb implementations that emit Ok/ConfigError/
// BadInput/DaemonRejected; for v1 we expose all six so the contract stays
// stable.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Ok = 0,
    Error = 1,
    DaemonNotRunning = 2,
    ConfigError = 3,
    BadInput = 4,
    DaemonRejected = 5,
}

impl ExitCode {
    pub fn into_raw(self) -> i32 {
        self as i32
    }
}

/// Print a message-formatted error to stderr in the
/// `scryd: <category>: <message>` shape and exit with the matching code.
pub fn bail(code: ExitCode, category: &str, message: &str) -> ! {
    eprintln!("scryd: {category}: {message}");
    std::process::exit(code.into_raw());
}
