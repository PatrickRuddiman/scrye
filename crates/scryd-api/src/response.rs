//! Shared response helpers — error envelope, common conversions.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::dto::{AddressDto, ErrorBodyDto, ErrorDto, MessageDto};

pub fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = ErrorDto {
        error: ErrorBodyDto {
            code: code.to_string(),
            message: message.to_string(),
        },
    };
    (status, Json(body)).into_response()
}

pub fn iso8601_from_unix(unix_secs: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(unix_secs, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0)
                .unwrap()
                .to_rfc3339()
        })
}

pub fn parse_iso_date(s: &str) -> Option<i64> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc().timestamp())
}

pub fn message_row_to_dto(row: scryd_storage::MessageRow) -> MessageDto {
    let to: Vec<AddressDto> = row.recipients_to.into_iter().map(addr_to_dto).collect();
    let cc: Vec<AddressDto> = row.recipients_cc.into_iter().map(addr_to_dto).collect();
    MessageDto {
        message_id: row.message_id,
        account_id: row.account_id,
        folder: row.folder,
        header_message_id: row.header_message_id,
        in_reply_to: row.in_reply_to,
        references: row.references,
        thread_id: row.thread_id,
        from: AddressDto {
            addr: row.sender_addr,
            name: row.sender_name,
        },
        to,
        cc,
        subject: row.subject,
        date: iso8601_from_unix(row.date_unix),
        body_md: row.body_md,
        // Attachment metadata persistence is out for v1 — the runtime sink
        // doesn't write attachment rows yet. The api still surfaces the
        // shape so callers don't have to special-case the field.
        attachments: Vec::new(),
    }
}

fn addr_to_dto(a: scryd_storage::Address) -> AddressDto {
    AddressDto {
        addr: a.addr,
        name: a.name,
    }
}
