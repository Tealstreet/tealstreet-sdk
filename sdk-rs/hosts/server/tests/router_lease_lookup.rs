mod support;
use safe_cex_exchange_server::auth::Verifier;
use std::time::Duration;

#[tokio::test]
async fn probe_socket_fence_check_does_not_register_unknown_users() {
    let app = support::app(
        support::config(Duration::from_secs(60)),
        Verifier::unsigned(),
    );
    assert_eq!(app.registry.router_lease_counts().tracked_users, 0);
    // What session::router_fenced asks on every socket-loop turn.
    for _ in 0..3 {
        let _ = app
            .registry
            .user_lease_fenced("viewer-never-starts-an-account");
    }
    assert_eq!(
        app.registry.router_lease_counts().tracked_users,
        0,
        "a read-only fence check registered an unknown user"
    );
}

#[tokio::test]
async fn unknown_socket_checks_keep_enforce_and_shadow_verdicts_without_tracking() {
    for enforce in [false, true] {
        let mut config = support::config(Duration::from_secs(60));
        config.router_lease.box_id = Some("01".into());
        config.router_lease.enforce = enforce;
        let app = support::app(config, Verifier::unsigned());
        for _ in 0..3 {
            assert_eq!(app.registry.user_lease_fenced("viewer"), enforce);
        }
        assert_eq!(app.registry.router_lease_counts().tracked_users, 0);
    }
}
