//! Mandatory venue-oracle:bybit CI step runs these with --include-ignored after root install.
#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn tables_and_origin_helpers_match_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-data.mjs"
        ))
        .output()
        .expect("Node and the workspace TypeScript dependency are required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn response_meta_matches_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-responses.mjs"
        ))
        .output()
        .expect("Node and the workspace TypeScript dependency are required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn api_and_site_helpers_match_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-helpers.mjs"
        ))
        .output()
        .expect("Node and workspace TypeScript required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn type_shapes_match_live_typescript_declarations() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-shapes.mjs"
        ))
        .output()
        .expect("Node and workspace TypeScript required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn simulator_build_configuration_matches_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-data.mjs"
        ))
        .env(
            "NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL",
            " HTTPS://sim.example/// ",
        )
        .env(
            "NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL",
            " wss://public.example/// ",
        )
        .env("NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL", "")
        .output()
        .expect("Node and workspace TypeScript required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn exchange_position_selection_matches_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-exchange-state.mjs"
        ))
        .output()
        .expect("Node and workspace TypeScript required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn execution_options_match_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-order-params.mjs"
        ))
        .output()
        .expect("Node and workspace TypeScript required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn batch_formatter_matches_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-order-format.mjs"
        ))
        .output()
        .expect("Node and workspace TypeScript required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn store_notification_source_finding() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-store-notification.mjs"
        ))
        .output()
        .expect("Node and workspace TypeScript required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
