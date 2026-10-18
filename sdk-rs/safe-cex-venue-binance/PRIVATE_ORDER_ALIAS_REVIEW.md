# Private and order capability review

This review owns the fifteen aliases assigned from the original Binance census.
The authenticated baseline produced fifteen individual missing-contract refusals.
Two aliases were reachable only through legacy test scaffolding and were removed;
thirteen live aliases have individual source-role contracts. No shared scanner,
generic family or other alias cohort is changed.

`acceptedAliasContext` obtains actual compiler declarations, source claims,
external artifacts and macro origins. The finite runner delegates native trait
and conversion detection to `runProbes`. Classification receipts are distinct
from the seven real native binding witnesses in `tests/support/private_order_aliases.rs`.
Only terminal HTTP, socket I/O and runtime clocks are controlled. The actual
owner, native imports, factories, signing, request scheduling, receiver bodies
and Store execute unchanged.

| Alias | Observed production path |
| --- | --- |
| `OperationFuture` | A held actual DELETE keeps cancellation pending and its order Open until release. |
| `Operation` | Actual amendment cancels before invoking the captured placement method. |
| `Delete` | The real signed DELETE retains symbol and the textual ID beyond the safe integer range. |
| `Sync` | The actual captured formatter produces the updated price, amount and LIMIT type. |
| `Status` | The real Store changes the selected order to Canceled after HTTP settlement. |
| `Post` | The batch path sends its actual serialized batchOrders in the signed terminal request. |
| `Classifier` | Mixed accepted, post-only and unrelated failure rows preserve accepted identity and exactly one error event. |
| `Log` | Actual close-position conflict cancellation publishes the exact prefixed message and info severity. |
| `RateHook` | The constructor installs the actual limiter: three private requests pass, the fourth waits for the clock window. |
| `NativeKeyFuture` | Actual key HTTP remains held while connecting, then continues after terminal release. |
| `RetryAction` | A failed real key request schedules a five-second reconnect; success installs refresh, disposal cancels it. |
| `PrivateHandler` | The actual captured Store callback preserves native signed-zero filled and exact price bits. |
| `EventArray` | The actual private parent returns native row fields and an Rc handle with stable clone identity. |

Every live alias has a separate production disconnection in
`scripts/private-order-alias-controls.py`. Each control must compile, fail its
named native assertion, restore the exact source bytes and pass. Compilation
errors and unexpected passes are refused. `EventArray` is a graph handle rather
than a Serde DTO; its non-decoder classification is still checked by the real
compiler probes. The typed Store snapshot does not claim mutable graph aliases
back into Store.

The removed `RefreshCallback` belonged only to `PrivateTimerState` and the two
standalone scheduling replicas. Their only callers were the old fake-host
oracle target; the native receiver already uses `PrivateState::schedule_refresh`
and `PrivateState::dispose`. Those duplicates and forty fake-only oracle cases
were retired. The unused PM detach copy was removed too: both real receivers
already call `detach_binance_private_socket`. All thirty-two source/native
detachment cases remain, now checking the one live helper for both variants.
Their target remains wired and no package script changes are needed.

The removed `KeyFuture` backed only the `ConnectBindings::fetch_native_key`
default wrapper and an overridden, unused `fetch_key` receiver method. The native
receiver already returns `NativeKeyFuture` directly. The trait now requires that
canonical native capability; the existing connection oracle implements it
without changing its source case inputs or expected effects.

Run these selected targets serially from `sdk-rs`:

```sh
node safe-cex-venue-binance/scripts/private-order-alias-contracts.mjs
cargo test --locked -p safe-cex-venue-binance --features native --jobs 1 --test private_classes private_order_aliases::
python3 safe-cex-venue-binance/scripts/private-order-alias-controls.py
cargo test --locked -p safe-cex-venue-binance --jobs 1 --test private_helpers -- --include-ignored
cargo test --locked -p safe-cex-venue-binance --jobs 1 --test private_session connect_
```

This finite native/default review does not exempt values passed through the
capabilities, other public aliases, arbitrary callbacks or the feature matrix.
Generated receipts and compiler snapshots remain under `target/`.

Initial measured receipts: thirteen authenticated native capability
classifications pass, separately from seven native binding tests. All thirteen
production faults compile and fail their named assertion, and every exact
restoration passes. The 32 live detach cases and all three connection groups pass;
strict native Clippy passes for the three touched integration targets. Required
all-features cancellation passes six tests, with only its explicit discovery
exporter ignored. Invention/status parity passes 7,563 rows and 16,696 public items.
These figures describe this finite checkpoint, not global behavioral equivalence.

Normal master merge `c45de1e1c9` retains both native test modules and every master
contract. The seven native binding tests and required cancellation six pass again;
invention/status parity passes 7,563 rows and 16,700 public Rust items. The six
owned declaration modules and thirteen controlled capability bodies are unchanged.
