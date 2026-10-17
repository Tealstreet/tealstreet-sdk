# Concrete Exchange binding

`native_exchange::AsterDEXExchange::new` assembles the existing constructor,
interceptors, REST workflows, clock and streams on `safe_cex_exchange::js_runtime`.
HTTP selection receives the exact primary/secondary options. The registry's
production host routes the final signed request through the common HTTP stack.

`tests/exchange_trait.rs` constructs the concrete exchange against a scripted
transport. Its all-sixteen witness calls start, get_account, validate_account,
fetch_positions, fetch_orders, fetch_markets, fetch_tickers,
fetch_symbol_max_leverage, set_leverage, change_position_mode,
change_margin_type, place_order, update_order, cancel_orders, schedule_cancel
and fetch_public_trades. It observes REST effects, decoded values and precision,
loaded Store flags and native socket startup. Separate witnesses cover secondary
read credentials/transport options, V1 HMAC, V3 ABI/code signing, auth-before-dial,
caller-ID preflight and one signature-rejection resend for both HTTP 200 and 400.

On 2026-10-16 each of the sixteen production trait overrides was independently
mutated to return a synthetic error before dispatch; schedule_cancel instead
returned success. For every mutation the all-sixteen witness exited 101 with
`test result: FAILED`, and no compilation failure. All sixteen were caught and
the exact production bytes restored before the passing gate. The controls do
not replace the source-oracle mutation witnesses already in the crate.

The complete scoped source-oracle run passed 186 tests in 23 targets, including
the source mutation witnesses. Native registry HTTP and constructor tests passed
13/13. The shared PlaceOrderOpts witness now observes impliedVolatility too
(325 measured source cases, 52 accepted, 18 fields).

Scoped commands (from repository root):

```sh
cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-asterdex --test exchange_trait --jobs 1
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-asterdex --all-targets --jobs 1 -- --include-ignored
cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --test native_host_asterdex --test rust_venues --jobs 1
```

The source inherits `BaseExchange.scheduleCancel`'s exact `Not implemented`
rejection (base.ts:989). Public trades preserve the existing declared refusal
of the source's unchecked uppercase BUY/SELL cast into the closed lower-case
OrderSide model. Representation costs remain in the owning manifest rows and
[canonical boundaries](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary).


Native hosted signer: the registry now threads its shared hosted account through
`rest_http_host_signed` for each Aster client, preserving the per-lane post-sign
transport adapter. BaseExchange and the JS source receiver retain that capability;
typed canSign and validateAccount observe it without serializing the runtime
handle. Structural options use the existing capability-stripping decoder.
The source still requires a nonempty API key; no local secret or V3 private key
is required with the hosted capability. Bare axios clock requests bypass signing.

`native_host_asterdex` checks local HMAC unchanged, hosted V1/V3 account reads and
validation with empty local secrets, account/exchange metadata, source API-key
refusal and unsigned clock HTTP during actual public startup. The hosted test
signer deliberately returns an independently verifiable HMAC query on both
routes: this proves capability/config transport, not a browser V3 signing
implementation. Existing actual-source V3 cryptographic witnesses keep that
separate scope. `scripts/asterdex-hosted-controls.py` in the registry disables
hosted capability, typed presence, credential bypass and bare-clock bypass
independently. All four faults compiled and failed named behavioral assertions
(exit101); every exact restoration passed. The initial new hosted witness also
compiled and failed the previous native-host refusal before production edits.


Hosted-signer readiness: registry 4/4, full venue including ignored live-source
oracles 186 tests in 23 targets, all-feature/ignored override contract 6/6,
strict venue host all-target lint, registry witness lint, WASM library lint and
scoped formatting all pass. Parity reports zero findings (7,459 rows, 17,181
public Rust items). Two reviewed Aster inventory body pins change: the globals
factory enclosing the unchanged clock.start call, and the typed canSign bridge.
No new base-delegation exception is needed.

Latest master refresh 71107baba8: native registry 4/4 and all-feature/ignored
override 6/6 pass; parity remains zero (7,459 rows; 17,183 public items).


## Strict mark-price ticker rows

`wrapped_mark_price_frame_retains_valid_siblings` and
`bare_mark_price_frame_retains_valid_siblings` compile against master fbee9d96aa
and fail at the first valid ETH sibling update. The fixed actual Exchange/API/
socket/Store path refuses complete BTC/string and LTC/boolean fundingTime rows,
keeps ETH/ADA updates and emits only their two mark-price events. Both refused
rows report their original symbol, field and value through the existing Error log.

`mark_price_falsey_time_and_unknown_targets_keep_source_behavior` preserves null,
false, zero and empty-string omission plus missing/inherited target no-ops. The
four existing production private-frame topic controls pass. The ignored actual
TypeScript public-dispatch comparison passes without changing its source corpus.
Official V3 API examples identify T as numeric next funding time; maintained TS
still controls the raw-copy/falsey behavior. Documentation checked at
https://github.com/asterdex/api-docs/blob/master/V3%28Recommended%29/EN/aster-finance-futures-api-v3.md#mark-price-stream-for-all-market.
Scoped strict Clippy, all-features cancellation inventory (6), override inventory,
parity and extractor gates pass. Independent review is clean; no shared API changes.
