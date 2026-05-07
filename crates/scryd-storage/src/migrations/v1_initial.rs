//! Migration v1: initial scryd schema.

pub const SQL: &str = r#"
CREATE TABLE accounts (
    account_id    TEXT    PRIMARY KEY,
    host          TEXT    NOT NULL,
    port          INTEGER NOT NULL,
    username      TEXT    NOT NULL,
    folders_json  TEXT    NOT NULL,
    active        INTEGER NOT NULL DEFAULT 1,
    mirrored_at   INTEGER NOT NULL
);

CREATE TABLE sync_state (
    account_id          TEXT    NOT NULL,
    folder              TEXT    NOT NULL,
    uidvalidity         INTEGER NULL,
    last_seen_uid       INTEGER NOT NULL DEFAULT 0,
    last_full_sync_at   INTEGER NULL,
    last_idle_at        INTEGER NULL,
    last_error          TEXT    NULL,
    account_health      TEXT    NOT NULL DEFAULT 'unknown',
    backoff_until       INTEGER NULL,
    PRIMARY KEY (account_id, folder),
    FOREIGN KEY (account_id) REFERENCES accounts(account_id)
);

CREATE TABLE messages (
    message_id          TEXT    PRIMARY KEY,
    account_id          TEXT    NOT NULL,
    folder              TEXT    NOT NULL,
    server_uid          INTEGER NOT NULL,
    uidvalidity         INTEGER NOT NULL,
    header_message_id   TEXT    NULL,
    in_reply_to         TEXT    NULL,
    references_json     TEXT    NULL,
    thread_id           TEXT    NOT NULL,
    sender_addr         TEXT    NOT NULL,
    sender_name         TEXT    NULL,
    recipients_to_json  TEXT    NULL,
    recipients_cc_json  TEXT    NULL,
    subject             TEXT    NULL,
    date_unix           INTEGER NOT NULL,
    raw_path            TEXT    NOT NULL,
    body_md             TEXT    NOT NULL,
    size_bytes          INTEGER NOT NULL,
    tombstoned_at       INTEGER NULL,
    UNIQUE (account_id, folder, server_uid, uidvalidity),
    FOREIGN KEY (account_id) REFERENCES accounts(account_id)
);

CREATE INDEX idx_messages_date    ON messages (date_unix DESC);
CREATE INDEX idx_messages_thread  ON messages (thread_id, date_unix ASC);
CREATE INDEX idx_messages_sender  ON messages (sender_addr, date_unix DESC);
CREATE INDEX idx_messages_folder  ON messages (folder, date_unix DESC);
CREATE INDEX idx_messages_account ON messages (account_id, date_unix DESC);

CREATE TABLE attachments (
    attachment_id  TEXT    PRIMARY KEY,
    message_id     TEXT    NOT NULL,
    filename       TEXT    NOT NULL,
    mime_type      TEXT    NOT NULL,
    size_bytes     INTEGER NOT NULL,
    FOREIGN KEY (message_id) REFERENCES messages(message_id)
);

CREATE INDEX idx_attachments_message ON attachments (message_id);

CREATE TABLE index_queue (
    message_id        TEXT    PRIMARY KEY,
    attempts          INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT    NULL,
    queued_at         INTEGER NOT NULL,
    failed_permanent  INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_index_queue_drainer ON index_queue (failed_permanent, queued_at ASC);
"#;
