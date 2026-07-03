use std::fs;
use std::path::Path;

use super::{FeedEvent, FeedMessage, parse_line};
use crate::model::{NotificationView, Urgency};

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/feed-fixtures"
    ))
    .to_path_buf()
}

fn parse_fixture(name: &str) -> Vec<FeedMessage> {
    let path = fixtures_dir().join(name);
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    raw.lines()
        .filter_map(|line| parse_line(line).expect("fixture line must parse"))
        .collect()
}

fn sample_notification() -> NotificationView {
    NotificationView {
        id: 1,
        app_id: "firefox".into(),
        summary: "Hello".into(),
        body: "World".into(),
        urgency: Urgency::Normal,
        timeout_ms: Some(10_000),
        has_actions: false,
    }
}

#[test]
fn parses_list_fixture() {
    let messages = parse_fixture("list.jsonl");
    assert_eq!(messages.len(), 1);
    let FeedMessage::Items(items) = &messages[0] else {
        panic!("expected items response");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, 1);
    assert_eq!(items[0].app_id, "firefox");
    assert_eq!(items[0].summary, "Hello");
    assert_eq!(items[0].body, "World");
    assert_eq!(items[0].urgency, Urgency::Normal);
    assert_eq!(items[0].timeout_ms, Some(10_000));
    assert!(!items[0].has_actions);
    assert_eq!(items[1].urgency, Urgency::Critical);
    assert!(items[1].has_actions);
}

#[test]
fn parses_subscribe_update_fixture() {
    let messages = parse_fixture("subscribe-update.jsonl");
    assert_eq!(messages.len(), 1);
    let FeedMessage::Event(FeedEvent::Update { items }) = &messages[0] else {
        panic!("expected update event");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], sample_notification());
}

#[test]
fn parses_subscribe_reload_fixture() {
    let messages = parse_fixture("subscribe-reload.jsonl");
    assert_eq!(messages.len(), 1);
    assert!(matches!(messages[0], FeedMessage::Event(FeedEvent::Reload)));
}

#[test]
fn ignores_subscribe_handshake_line() {
    let line = r#"{"v":1,"cmd":"subscribe"}"#;
    assert!(parse_line(line).unwrap().is_none());
}

#[test]
fn empty_line_is_ignored() {
    assert!(parse_line("").unwrap().is_none());
    assert!(parse_line("   ").unwrap().is_none());
}

#[test]
fn rejects_unknown_message_type() {
    let err = parse_line(r#"{"v":1,"type":"pong"}"#).unwrap_err();
    assert!(err.to_string().contains("unknown message type"));
}

#[test]
fn rejects_unknown_urgency() {
    let line = r#"{"v":1,"type":"items","items":[{"id":1,"app_id":"x","summary":"s","urgency":"urgent"}]}"#;
    let err = parse_line(line).unwrap_err();
    assert!(err.to_string().contains("unknown urgency"));
}
