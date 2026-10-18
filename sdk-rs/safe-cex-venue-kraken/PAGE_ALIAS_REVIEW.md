# Kraken income-page alias cohort

`private_accounts::PageFuture<'static, binding::rt::Rt>` is the boxed native
completion `Pin<Box<dyn Future<Output = Result<Rc<Js>, Thrown>> + 'static>>`.
`PageCallback<'static, Rt>` is the boxed callable taking `Rc<Js>` and returning
`Result<PageFuture<'static, Rt>, Thrown>`. These are two distinct continuation
capabilities, not wire decoders. The reviewed profiles fix only the real
`Scope<KrakenExtra>` host and this lifetime; other instantiations remain open.
The profile expected types are authored independently of the compiler targets.

`scripts/page-alias-contracts.mjs` obtains fresh compiler JSON, artifacts, macro
origins, external-root identities and public declaration claims through the
maintained `acceptedAliasContext`, then runs the stock native alias detectors.
`--expect-unclassified` requires an actual missing-contract failure for both
owners. Classification does not count as executing the pager or as proving
the remaining Kraken aliases.

`tests/support/accepted_page_aliases.rs` uses the Scope retained by a constructed
production Futures exchange. It boxes the real `IncomeHost::account_log_page`
future, holds and releases its terminal HTTP response, and checks request
cardinality/parameters plus two independent native income amounts. A native
property-getter failure retains the original object and UTF16/-0 cells before
the future exists; a real HTTP refusal rejects after the future is created.
The actual `private_accounts::income_range` additionally executes its production
callback through the shared pager and signed history client, checking the
source window lower bound and 500-row limit without an upper-bound wire field.

The TypeScript reference is `kraken.exchange.ts::_fetchAccountLogPage` and
`fetchIncomeHistoryRange`: the callback delegates `{startTime, limit}`, the
request emits only `{since, count}`, and refusal envelopes throw. No production
body or host behavior changes in this cohort. The terminal fixture's optional
response latch defaults to released and preserves the existing requests/routes.

`scripts/page-alias-controls.py` changes the actual page producer to discard its
completed rows, then separately changes the production pager's limit. Each
fault must compile, fail its own named native assertion, restore exact source
bytes and pass again. Compiler errors never count as negative controls.

Commands run serially from `sdk-rs`:

```sh
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/page-alias-contracts.mjs --expect-unclassified
cargo test --locked -p safe-cex-venue-kraken --jobs 1 --lib accepted_page_aliases
python3 safe-cex-venue-kraken/scripts/page-alias-controls.py
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/page-alias-contracts.mjs
RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR="$PWD/target/accepted-inventory" node safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-kraken --inventory --jobs=1
```

Rustdoc/check discovery stays in `target/accepted-inventory`, separate from the
ordinary native builds. The full parity index and all-features cancellation
inventory are separate readiness gates.

The selected alias replay reports two explicit classifications, zero failed
owners and zero decoder runtime results. All three constructed-binding tests
pass. Both semantic faults compile and fail their named assertions; each exact
source restoration passes. The two original missing-contract receipts remain
independent of the semantic fault controls.

The full canonical Kraken inventory clears both page profiles and advances to
`construction::Factory`, which still requires its own finite generic profile.
It remains an explicit following-cohort obligation; no full-Kraken inventory
pass is claimed here. Nineteen of the original 24 aliases remain after this
cohort and the three earlier `rest_account` aliases.
