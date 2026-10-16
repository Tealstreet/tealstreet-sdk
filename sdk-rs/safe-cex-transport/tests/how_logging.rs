#[test]
fn native_log_probe_emits_sanitized_records_only_when_enabled() {
    for (debug, public, enabled) in [
        (None, None, false),
        (Some("0"), None, false),
        (Some("1"), None, true),
        (Some("true"), None, true),
        (None, Some("1"), true),
    ] {
        let mut command = std::process::Command::new(env!("CARGO"));
        command
            .args(["run", "--quiet", "--locked", "--example", "how_log_probe"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env_remove("DEBUG")
            .env_remove("NEXT_PUBLIC_LOG_TRADETUNNEL_TO_CONSOLE_LOG");
        if let Some(value) = debug {
            command.env("DEBUG", value);
        }
        if let Some(value) = public {
            command.env("NEXT_PUBLIC_LOG_TRADETUNNEL_TO_CONSOLE_LOG", value);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        if !enabled {
            assert!(stdout.is_empty(), "{stdout}");
            continue;
        }
        assert!(stdout.contains("[TradeTunnel] REQUEST:"));
        assert!(stdout.contains("[TradeTunnel] RESPONSE:"));
        assert!(stdout.contains("[REDACTED]"));
        assert!(stdout.contains("selectedRegion"));
        for forbidden in ["hidden", "secret", "headers", "Authorization"] {
            assert!(!stdout.contains(forbidden), "{stdout}");
        }
    }
}
