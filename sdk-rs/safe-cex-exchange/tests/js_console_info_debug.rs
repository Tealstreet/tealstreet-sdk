//! Shared console methods deliver through the actual NativeRuntime sink.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{client, globals, scope::ScopeShared, value::Js},
    native::native_runtime,
};
use std::{process::Command, rc::Rc, sync::Arc};

fn native_console_info_debug_child() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            runtime,
            http,
            Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let console = shared.globals.get("console").unwrap().clone();
        for (method, n) in [("info", 17.), ("debug", 23.)] {
            let result = console
                .call_method(
                    method,
                    vec![Js::str(&format!("M8-{method}")), Js::Number(n)],
                )
                .unwrap();
            assert!(result.is_undefined(), "console.{method} result");
        }
    });
}
#[test]
fn native_console_info_debug_reach_production_sink_like_node() {
    if std::env::var("SAFE_CEX_CONSOLE_INFO_DEBUG_CHILD").as_deref() == Ok("1") {
        native_console_info_debug_child();
        return;
    }
    let source = Command::new("node")
        .args([
            "-e",
            "console.info('M8-info',17);console.debug('M8-debug',23)",
        ])
        .output()
        .unwrap();
    assert!(source.status.success());
    let expected = String::from_utf8(source.stdout).unwrap();
    assert_eq!(expected.lines().count(), 2);
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native_console_info_debug_reach_production_sink_like_node",
            "--nocapture",
        ])
        .env("SAFE_CEX_CONSOLE_INFO_DEBUG_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "actual native console child failed: {}{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    let stderr = String::from_utf8(child.stderr).unwrap();
    let mut output = vec![];
    for (method, line) in ["info", "debug"].into_iter().zip(stderr.lines()) {
        output.push(
            line.strip_prefix(&format!("[safe-cex {method}] "))
                .unwrap_or_else(|| panic!("native severity mismatch: {line}")),
        );
    }
    assert_eq!(
        stderr.lines().count(),
        2,
        "both calls reached actual native sink"
    );
    assert_eq!(
        output.join("\n") + "\n",
        expected,
        "native console values match Node"
    );
}
