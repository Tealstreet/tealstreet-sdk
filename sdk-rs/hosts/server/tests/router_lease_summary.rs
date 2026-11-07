mod support;
use safe_cex_exchange_server::auth::Verifier;
use serde_json::Value;
use std::time::Duration;

#[tokio::test]
async fn summary_route_is_on_box_count_only_and_never_creates_lease_records() {
    let mut config = support::config(Duration::from_secs(60));
    config.router_lease.box_id = Some("01".into());
    let app = support::app(config, Verifier::unsigned());
    let socket = app.registry.user_drains.socket("private-live-sentinel");
    app.registry
        .router_leases
        .get("private-tracked-sentinel")
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = safe_cex_exchange_server::http::router(app.clone());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::new();
    let url = format!("http://{address}/admin/router-lease/summary");
    let status_url = format!("http://{address}/admin/router-lease/status");
    for _ in 0..2 {
        let response = client
            .post(&status_url)
            .body(serde_json::json!({"userId":"private-query-sentinel"}).to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(
            app.registry.router_lease_counts().tracked_users,
            1,
            "read-only status must not register an unknown user"
        );
        assert_eq!(response.status(), 503);
    }
    assert_eq!(
        app.registry.router_lease_counts().tracked_users,
        1,
        "read-only status must not register an unknown user"
    );
    for body in ["", "{}"] {
        let response = client.post(&url).body(body).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-origin")
        );
        let text = response.text().await.unwrap();
        assert!(
            !text.contains("private-live-sentinel") && !text.contains("private-tracked-sentinel")
        );
        let summary: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(summary.as_object().unwrap().len(), 11);
        assert_eq!(summary["capacity"], 4096);
        assert_eq!(summary["remaining"], 4095);
        assert_eq!(summary["routerBoxId"], "01");
        assert_eq!(summary["enforced"], false);
        assert!(summary.get("enableGraceUntil").unwrap().is_null());
        assert_eq!(summary["trackedUsers"], 1);
        assert_eq!(summary["fencedUsers"], 0);
        assert_eq!(summary["wouldFenceUsers"], 1);
        assert_eq!(summary["liveUsersWithoutLease"], 1);
        assert_eq!(
            summary["liveWouldFenceUsers"], 0,
            "a tracked user with no live socket or account is departed, not at risk"
        );
        assert!(summary["checkedAt"].as_u64().unwrap() > 0);
    }
    for (name, value) in [
        ("cf-connecting-ip", "127.0.0.1"),
        ("x-forwarded-for", "127.0.0.1"),
    ] {
        assert_eq!(
            client
                .post(&url)
                .header(name, value)
                .body("{}")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    }
    for body in [
        "[]",
        "null",
        "{broken",
        r#"{"userId":"private-query-sentinel"}"#,
    ] {
        assert_eq!(
            client.post(&url).body(body).send().await.unwrap().status(),
            400
        );
    }
    assert_eq!(client.get(&url).send().await.unwrap().status(), 405);
    assert_eq!(
        client
            .post(&url)
            .body(" ".repeat(1025))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
    drop(socket);
    assert_eq!(app.registry.router_lease_counts().tracked_users, 1);
    assert_eq!(
        app.registry.router_lease_counts().live_users_without_lease,
        0
    );
    server.abort();
}
