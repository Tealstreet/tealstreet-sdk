//! Native account timestamps must be legal JavaScript Date.now/BigInt nonces.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{native::native_runtime, runtime::ExchangeRuntime};

#[test]
fn native_wall_clock_supplies_integer_epoch_milliseconds() {
    let runtime = native_runtime();
    for _ in 0..16 {
        let now = runtime.now_ms();
        assert!(now.is_finite() && now > 0.0);
        assert_eq!(
            now.fract(),
            0.0,
            "native wall-clock timestamp must be a valid integer nonce: {now}"
        );
    }
}
