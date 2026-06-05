//! Public option parsing and error display behavior.

use kamu_logging::{Error, Format, Sink};

#[test]
fn format_from_env_value_accepts_supported_values() {
    assert_eq!(Format::from_env_value("compact"), Format::Compact);
    assert_eq!(Format::from_env_value("PRETTY"), Format::Pretty);
    assert_eq!(Format::from_env_value(" json "), Format::Json);
}

#[test]
fn format_from_env_value_falls_back_to_auto() {
    assert_eq!(Format::from_env_value(""), Format::Auto);
    assert_eq!(Format::from_env_value("unknown"), Format::Auto);
}

#[test]
fn sink_from_env_value_accepts_supported_values() {
    assert_eq!(Sink::from_env_value("stdout"), Sink::Stdout);
    assert_eq!(Sink::from_env_value("STDERR"), Sink::Stderr);
    assert_eq!(Sink::from_env_value(" journald "), Sink::Journald);
}

#[test]
fn sink_from_env_value_falls_back_to_auto() {
    assert_eq!(Sink::from_env_value(""), Sink::Auto);
    assert_eq!(Sink::from_env_value("console"), Sink::Auto);
}

#[test]
fn invalid_configuration_error_is_actionable() {
    let err = Error::InvalidConfiguration("Sink::Journald is unavailable".to_owned());
    assert_eq!(err.to_string(), "invalid logging configuration: Sink::Journald is unavailable",);
}
