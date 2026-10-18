# Final seven Binance FIX-audit rows

The three fetchMarkets rows retain their approved strict-row divergence. The
four unused generated position/income codecs are deleted, as explicitly directed
by the coordinator after checking both Rust repositories. This is retirement of
unused code, not approval of the shared 128-level numeric-capture bound.
`tests/divergence-rulings.json` records all seven individual dispositions.

## Real production paths

| Assigned source row | Disposition | Live Rust path |
| --- | --- | --- |
| BinanceExchange.fetchMarkets | ACCEPT approved strict rows | ExchangeState::fetch_markets -> fetch_futures_markets |
| BinancePMExchange.fetchMarkets | ACCEPT approved strict rows | ExchangeState::fetch_markets -> fetch_pm_markets |
| BinanceSpotExchange.fetchMarkets | ACCEPT approved strict rows | ExchangeState::fetch_markets -> fetch_spot_markets |
| binance GET_POSITION_RISK_RESPONSE | DELETE dead codec | position_snapshots::fetch_futures_positions |
| binancepm GET_POSITION_RISK_RESPONSE | DELETE dead codec | position_snapshots::fetch_pm_positions |
| binance BinanceIncomeRecord | DELETE dead codec | rest_history::income_page / income_history |
| binance BinanceIncomeHistoryOptions | DELETE dead codec | rest_history::income_history |

The market authorization is the coordinator's `known-residuals.md` decisions
“Binance family strict market asset rows” and “remaining Binance Market scalar
fields.” Raw source projections retain original values. Concrete typed admission
reports and omits only bad rows after the entire source projection; valid siblings
remain in order and genuine source/transport errors retain Store fallback.
The existing native tests construct NativeImports, actual API factories, signer,
ExchangeOwner and SDK class, scripting only terminal HTTP. They cover 60 asset
and 46 symbol/leverage fetch/refresh paths, plus the actual TS source's 30 asset
and 23 scalar rows. These tests and every surviving identity guard are retained.

## Both-repository dead-code proof

Searched this mirror at base `5c3082cee1e8e82168af2aacf18eaaa0cf363abb` and
tealstreet-next at `65e086a3c9039a79e4c7e9f354aba31d8f83009e`. The only external
embedder uses safe-cex-ffi (exchange-server/connect-edge). Neither Rust repository
has a production use of these four codecs outside their own generated declarations.
The TypeScript imports/generic annotations still exist and erase at runtime;
they are not consumers of a Rust decoder.

Reproduce the census in each repository with:

```sh
rg --color never -n --type rust '\b(GetPositionRiskResponse(Row)?|BinanceIncomeRecord|BinanceIncomeHistoryOptions)\b' .
```

Before deletion, all Rust matches belong to the three generated family files or
their tests. After deletion, this mirror has nine matching test locations for the
separate inverse cohort; next has 62 matching declaration/test locations. No
other Rust source location matches. The inverse cohort is outside this assignment
and remains byte-unchanged. Full Binance compilation after deleting the codecs
is the compiler proof that the live implementation never required them.

`types.mjs` explicitly retires exactly the four source annotations while checking
that each still exists. It emits no stand-in alias or replacement DTO. Generated
Futures/PM declarations and the six record/vector accepted-set contracts are
removed. Only their obsolete standalone fixtures and control runners disappear.
The shared numeric buffer, raw parser, surviving income union, inverse codecs,
map/string guards, and unrelated contract bytes (including negative zero) stay
unchanged. Historical field-identity/gate receipts are labeled as historical.

## Constructed binding probes and controls

The new `private_classes::lifecycle_aliases::native_final_audit_position_readers_keep_fields_without_generated_dtos`
executes both real SDK fetch_positions methods using NativeImports and signed
HTTP. It independently reads symbol/side and distinct entry/contracts/notional/
leverage/liquidation values, signed-zero PnL bits, and the retained original
wire info. It proves these live readers survive the declaration removal.
Existing lifecycle and factory probes execute real income pages/ranges and actual
signed option forwarding, including source -0/NaN/infinite income profiles.

Run from the root, serially with no other Cargo build in this worktree:

```sh
CARGO_INCREMENTAL=0 python3 sdk-rs/safe-cex-venue-binance/scripts/final-fix-audit-controls.py
```

Five independent production faults bypass typed market admission, corrupt the
Futures position result, corrupt the PM position result, corrupt income output,
and corrupt forwarded income limit. Each must compile and fail its named runtime
assertions, restore the exact original bytes, and pass the same tests again.
All five controls compiled RED and passed after exact restoration;
FINAL_FIX_AUDIT_CONTROLS.json records the changed expressions, named tests and
restored production hashes. No temporary fault is committed. No shared bound claim or synthetic payload
maximum is used to close any row.

## Validation

Final gates and compiled-control results are recorded here after completion.
