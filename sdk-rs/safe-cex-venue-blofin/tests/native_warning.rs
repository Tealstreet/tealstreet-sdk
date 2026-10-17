//! Public connection-failure notice through the production venue, host and timer.
#[allow(dead_code)]
#[path = "support/native_runtime.rs"]
mod support;

use safe_cex_exchange::exchange::Exchange;
use serde_json::json;
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[test]
fn connection_failure_notice_preserves_both_listener_arguments() {
    let (venue, rt, _, wire) = support::fixture();
    for (path, data) in [
        ("/api/v1/account/margin-mode", json!({"marginMode":"cross"})),
        (
            "/api/v1/account/position-mode",
            json!({"positionMode":"net_mode"}),
        ),
    ] {
        wire.bodies
            .borrow_mut()
            .insert(path.into(), json!({"code":"0", "data":data}));
    }
    let warnings = Rc::new(RefCell::new(Vec::new()));
    let seen = warnings.clone();
    venue.base().on_args(
        "warning",
        Rc::new(move |_, args| {
            let args: Vec<serde_json::Value> = args
                .iter()
                .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                .collect();
            seen.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    let initial = rt.now.get();
    {
        let mut start = std::pin::pin!(venue.start());
        let mut settled = false;
        for _ in 0..128 {
            if let Poll::Ready(result) =
                start.as_mut().poll(&mut Context::from_waker(Waker::noop()))
            {
                result.unwrap();
                settled = true;
                break;
            }
            rt.pump();
        }
        assert!(
            settled,
            "startup settles with immediate scripted REST replies"
        );
    }
    assert!(
        rt.dials
            .borrow()
            .iter()
            .any(|url| url.ends_with("/ws/public"))
    );
    assert!(warnings.borrow().is_empty());
    rt.now.set(initial + 19_999.0);
    rt.pump();
    assert!(warnings.borrow().is_empty(), "notice waits twenty seconds");
    rt.now.set(initial + 20_000.0);
    rt.pump();
    // blofin.ws-public.ts armConnectionFailureNotice: emitter.emit('warning', NOTICE, {forceToast:true}).
    assert_eq!(
        *warnings.borrow(),
        vec![vec![
            json!(
                "BloFin live WebSocket connection failed to establish. Historical candles may still load, but live charts and order books will not update. Check your network, VPN, firewall, or browser extensions, then reload Tealstreet."
            ),
            json!({"forceToast":true}),
        ]],
        "warning listeners receive the source message and options as separate arguments"
    );
    rt.now.set(initial + 60_000.0);
    rt.pump();
    assert_eq!(warnings.borrow().len(), 1, "notice fires once");
    venue.dispose().unwrap();
}
