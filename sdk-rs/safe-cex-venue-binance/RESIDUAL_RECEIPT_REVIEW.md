# Four caller-bound Binance helpers

This review classifies four compiler-discovered free-function owners with
executed native observations, rather than diagnostic or exemption records:

| Owner | Native observations | Boundary |
| --- | ---: | --- |
| `account_leverage_overlay::build_overlay` | 10 | Decoded bracket rows applied to real `Market` records |
| `closed_position_reader::derive_binance_closed_positions` | 13 | Typed options and terminal request/error-emitter ports |
| `lifecycle::loaded` | 7 | Canonical `Js` values through a concrete `LifecycleBindings` port |
| `private_helpers::detach_binance_private_socket` | 14 | Captured socket state and fallible native callback ports |

The assigned fifth owner, `detach_binance_pm_private_socket`, is a duplicate
tail deleted by merged #938 (`70ebaee37a`). Its source mapping and
maintained-source cases now use the surviving shared helper. This review adds
no duplicate contract for that retired declaration. Public callable aliases remain with their existing
owners, and the separate `closed_pnl` mapper is outside this cohort.

The adapters in `scripts/residual-receipt-observers.rs` receive the actual
compiler-selected function as `$type`. They script its declared caller ports;
they do not replace its transformation, sort, range splitting, shallow copy or
detach implementation. Contract outputs are independently authored. Native
number bits, reference identity, own undefined and unchanged input fields are
checked before producing the finite JSON observation.

The leverage profile covers stock decoded JSON and the production typed Market
adapter. It does not claim arbitrary JavaScript keys, executable coercion hooks
or reference identity for cloned Market DTOs. The reader observes its orchestration
and selected native output fields; the full closed-PnL mapping has its own corpus.
The loaded profile covers decoded data, primitive spread and shallow native graph
references, not arbitrary accessors or Symbol keys. Detachment observes its
fallible callback boundary, not unrestricted JavaScript method lookup. These
limits preserve the existing source/body boundary rather than claiming broader
class equivalence.

The existing maintained-source oracles remain unchanged:
`tests/rest_reconciliation.rs`, `tests/closed_position_reader.rs`,
`tests/lifecycle.rs` and `tests/private_helpers.rs`. They exercise the source
methods and retain the wider malformed-input, scheduling and failure corpus.

Run the maintained, compiler-authenticated measurement driver from `sdk-rs`:

```sh
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR="$PWD/target/accepted-inventory" \
  node safe-cex-parity/scripts/accepted-set.mjs \
  --package=safe-cex-venue-binance --jobs=1 --measure \
  --owner=account_leverage_overlay::build_overlay \
  --owner=closed_position_reader::derive_binance_closed_positions \
  --owner=lifecycle::loaded \
  --owner=private_helpers::detach_binance_private_socket
```

`scripts/residual-receipt-controls.py` runs that same maintained command; it
does not implement a second discovery or probe filter. It requires four native
execution receipts, then independently changes the leverage maximum, reader
sort direction, loaded flag and detach event. Each changed body must compile
and fail its named receipt while the other three stay green. Exact production
bytes are restored in `finally`, and all four must pass again after each fault.
Compilation errors, missing receipts and unexpected passes are rejected.
