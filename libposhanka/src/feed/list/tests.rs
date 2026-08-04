use super::parse_list_output;

#[test]
fn parses_wire_list_line() {
    let raw = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/feed-fixtures/list.jsonl"
    ));
    let items = parse_list_output(raw.trim()).expect("wire list");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].summary, "Hello");
}

#[test]
fn parses_json_array_like_notredctl() {
    let raw = r#"[
  {
    "id": 3,
    "app_id": "firefox",
    "summary": "Hi",
    "body": "there",
    "urgency": "normal",
    "timeout_ms": 5000,
    "has_actions": false
  }
]"#;
    let items = parse_list_output(raw).expect("array list");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, 3);
    assert_eq!(items[0].summary, "Hi");
}

#[test]
fn empty_stdout_is_empty_list() {
    assert!(parse_list_output("").unwrap().is_empty());
}
