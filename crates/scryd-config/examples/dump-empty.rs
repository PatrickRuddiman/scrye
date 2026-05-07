//! Demonstrates that AccountPassword's Debug never leaks the secret.

use scryd_config::AccountPassword;

fn main() {
    let pw = AccountPassword::new("never-print-me".to_string());
    println!("{pw:?}");
}
