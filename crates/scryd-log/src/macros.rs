//! Categorized log macros. Every other crate uses these instead of calling
//! `tracing::event!` directly so the closed-set fields stay grep-able.

/// Emit a categorized failure event. Default level ERROR; pass
/// `severity = warn` (or `severity = error`) as the leading argument to
/// override.
///
/// ```ignore
/// scryd_log::log_failure!(category = scryd_log::category::AUTH_REJECTION, account_id = "primary");
/// scryd_log::log_failure!(severity = warn, category = scryd_log::category::SINGLE_MESSAGE_PARSE_FAILURE, message_id = id);
/// ```
#[macro_export]
macro_rules! log_failure {
    (severity = error, $($rest:tt)*) => {
        ::tracing::event!(::tracing::Level::ERROR, $($rest)*)
    };
    (severity = warn, $($rest:tt)*) => {
        ::tracing::event!(::tracing::Level::WARN, $($rest)*)
    };
    ($($rest:tt)*) => {
        ::tracing::event!(::tracing::Level::ERROR, $($rest)*)
    };
}

/// Emit a non-failure lifecycle event. Default level INFO; pass
/// `severity = debug` (etc.) to override.
#[macro_export]
macro_rules! log_lifecycle {
    (severity = error, $($rest:tt)*) => {
        ::tracing::event!(::tracing::Level::ERROR, $($rest)*)
    };
    (severity = warn, $($rest:tt)*) => {
        ::tracing::event!(::tracing::Level::WARN, $($rest)*)
    };
    (severity = debug, $($rest:tt)*) => {
        ::tracing::event!(::tracing::Level::DEBUG, $($rest)*)
    };
    (severity = info, $($rest:tt)*) => {
        ::tracing::event!(::tracing::Level::INFO, $($rest)*)
    };
    ($($rest:tt)*) => {
        ::tracing::event!(::tracing::Level::INFO, $($rest)*)
    };
}

/// Emit an api-request access-log event at DEBUG, always tagged
/// `kind = "request"`. Caller supplies `method`, `path`, `status`,
/// `duration_ms`.
#[macro_export]
macro_rules! log_request {
    ($($rest:tt)*) => {
        ::tracing::event!(
            ::tracing::Level::DEBUG,
            kind = $crate::kind::REQUEST,
            $($rest)*
        )
    };
}
