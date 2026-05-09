//! Shared test helpers for live integration tests against a running
//! GreenMail container. Each test that depends on the fixture starts
//! with `support::greenmail::skip_if_unreachable()` so CI and local
//! runs without the fixture pass silently.

#![allow(dead_code)]

pub mod greenmail;
pub mod test_sink;
