use super::{CommandError, run_command};
use crate::feed::ProviderSpec;
use std::os::unix::fs::PermissionsExt;

#[test]
fn run_command_requires_binary() {
    let spec = ProviderSpec::default();
    let err = run_command(&spec, &["list"]).unwrap_err();
    assert!(matches!(err, CommandError::NoCommand));
}

#[test]
fn run_command_passes_socket_flag() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("mockctl");
    std::fs::write(
        &script,
        r#"#!/bin/sh
while [ $# -gt 0 ]; do
  case "$1" in
    --socket) shift 2 ;;
    list) echo '[]'; exit 0 ;;
    *) shift ;;
  esac
done
exit 1
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();

    let spec = ProviderSpec {
        command: Some(script.to_string_lossy().into_owned()),
        socket: Some("/tmp/test.sock".into()),
        ..Default::default()
    };
    let out = run_command(&spec, &["list"]).expect("mock list");
    assert_eq!(out.trim(), "[]");
}
