# WOO public async call boundary

The public `WooExchange` methods share `invoke` and `invoke_unit`. Both used to
look up and call the configured class member inside a lazy Rust future. Calling
the SDK without polling its returned future therefore did nothing. Dropping that
future prevented requests and their catch/report continuations entirely.

The maintained `WOOXExchange` class runs each async member's synchronous prefix
at invocation. Its Promise continues independently of whether the caller awaits
it. WOO belongs to the existing-JavaScript-Promise family in the public wrapper
census, rather than the typed-native-body family that uses `start_eager`.

Both wrappers now capture the real member's `Result<Js, Thrown>` before boxing
the waiter. They await that same original Promise with the existing runtime.
There is no additional spawn, Promise, driver or per-method execution path.
Typed decoding still happens after fulfillment, and lookup errors still reject
the returned future through the existing error conversion.

## Native and source observations

`tests/support/eager_binding.rs` runs the actual constructed venue through the
public `Exchange` trait, its configured class, Axios/signers and terminal HTTP:

- Both wrappers look up their existing member immediately. A getter returns the
  unchanged original function; its body and operation table are not replaced.
- An unpolled `fetch_markets` waiter can be dropped while the public HTTP response
  is held. Releasing it still runs the real catch and emits the source error.
- An unpolled `cancel_all_orders` waiter behaves likewise through the actual
  signed DELETE, preserving the authentication header and exactly one report.
- Ready results, a held waiter and the existing empty-market fallback remain
  observable through the returned future.

`scripts/eager-binding-source.cjs` constructs the whole maintained TypeScript
class. Only terminal clients, socket construction and the base host are supplied.
The original `fetchMarkets` and `cancelAllOrders` bodies prove the immediate
member read/request and detached catch. Endpoint values and utility code come
from their maintained source declarations.

The exact master bodies must fail four named native assertions: both immediate
member reads and both unpolled-drop continuations. Source and awaited-result
controls pass on that same baseline. This witnesses the shared wrapper timing;
it does not close unrelated history decoding, metadata or inactive V3 rows.

Run from `sdk-rs` with `SAFE_CEX_TYPESCRIPT_PATH` pointing to the installed compiler:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-woo --lib eager_binding_tests:: -- --nocapture
```
