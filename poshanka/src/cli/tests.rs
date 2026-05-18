use super::Cli;
use clap::Parser;

#[test]
fn parses_defaults() {
    Cli::try_parse_from(["poshanka"]).unwrap();
}

#[test]
fn parses_config_and_theme_paths() {
    Cli::try_parse_from([
        "poshanka",
        "--config",
        "/tmp/c.toml",
        "--theme",
        "/tmp/t.toml",
    ])
    .unwrap();
}
