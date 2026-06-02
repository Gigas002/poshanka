use super::Cli;
use clap::Parser;

#[test]
fn parses_defaults() {
    Cli::try_parse_from(["poshanka"]).unwrap();
}

#[test]
fn parses_config_path() {
    let cli = Cli::try_parse_from(["poshanka", "--config", "/tmp/c.toml"]).unwrap();
    assert_eq!(
        cli.config.as_deref(),
        Some(std::path::Path::new("/tmp/c.toml"))
    );
}

#[test]
fn rejects_unknown_theme_flag() {
    assert!(Cli::try_parse_from(["poshanka", "--theme", "/tmp/t.toml"]).is_err());
}
