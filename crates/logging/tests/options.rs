//! Public option parsing and error display behavior.

#![cfg(any(feature = "systemd", feature = "wasm32"))]

use kamu_logging::{Error, Format, Sink};

#[test]
fn format_from_env_value_accepts_supported_values() {
    assert_eq!(Format::from_env_value("auto"), Ok(Format::Auto));
    assert_eq!(Format::from_env_value("compact"), Ok(Format::Compact));
    assert_eq!(Format::from_env_value("PRETTY"), Ok(Format::Pretty));
    assert_eq!(Format::from_env_value(" json "), Ok(Format::Json));
}

#[test]
fn format_from_env_value_rejects_unknown_values() {
    assert!(Format::from_env_value("").is_err());
    assert!(Format::from_env_value("unknown").is_err());
}

#[test]
fn sink_from_env_value_accepts_supported_values() {
    assert_eq!(Sink::from_env_value("auto"), Ok(Sink::Auto));
    assert_eq!(Sink::from_env_value("stdout"), Ok(Sink::Stdout));
    assert_eq!(Sink::from_env_value("STDERR"), Ok(Sink::Stderr));
    assert_eq!(Sink::from_env_value(" journald "), Ok(Sink::Journald));
}

#[test]
fn sink_from_env_value_rejects_unknown_values() {
    assert!(Sink::from_env_value("").is_err());
    assert!(Sink::from_env_value("console").is_err());
}

#[test]
fn invalid_configuration_error_is_actionable() {
    let err = Error::InvalidConfiguration("Sink::Journald is unavailable".to_owned());
    assert_eq!(err.to_string(), "invalid logging configuration: Sink::Journald is unavailable",);
}

#[test]
fn invalid_environment_error_does_not_echo_rejected_value() {
    let err = Error::InvalidEnvironmentValue {
        variable: "KAMU_LOG_SINK".to_owned(),
        expected: "one of: auto, stdout, stderr, journald",
    };
    assert_eq!(err.to_string(), "invalid KAMU_LOG_SINK: expected one of: auto, stdout, stderr, journald");
}

#[cfg(feature = "with-otlp")]
#[test]
fn init_options_debug_inherits_otlp_redaction() {
    use kamu_logging::InitOptions;
    use kamu_logging::otlp::OtlpConfig;

    let marker = "do-not-print-this-credential";
    let options = InitOptions::default().with_otlp(
        OtlpConfig::new(format!("https://user:{marker}@collector.invalid"))
            .with_header("authorization", marker)
            .with_resource_attribute("service.credential", marker),
    );

    let debug = format!("{options:?}");
    assert!(!debug.contains(marker));
    assert!(debug.contains("<redacted>"));
}
