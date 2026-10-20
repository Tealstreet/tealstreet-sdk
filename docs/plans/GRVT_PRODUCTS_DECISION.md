# GRVT Future/Call/Put product decision

Branch `feat/rust-grvt-products`, from `origin/feat/rust-rewrite` at
`bf1cf3c99a`. PR target `feat/rust-rewrite`; full auto, no stash/rebase/force
push, no TS venue deletion. Follow `VENUE_PORT_BRIEF.md`.

## Decision for Sam

Retire the latent **mapper-only Future/Call/Put surface** from the Rust cutover
parity requirements. Retain GRVT's supported `PERPETUAL` product. This is a
supported-product decision, not a claim that GRVT's protocol cannot represent
futures/options or that its generic TS order builder rejects those kinds.
Keep TS implementations and public builder-wallet exports. Do not enable new
products or default cutover as a consequence of this decision.

TS `fetchMarkets()` posts `{}` to `/lite/v1/all_instruments`. The current
[official Get All Instruments contract](https://api-docs.grvt.io/market_data_api/#get-all-instruments)
defaults omitted **or empty** `k` to `PERPETUAL`. The TS mapper accepts any
returned row, marking only `PERPETUAL` as perpetual, but never requests the
other three kinds. Its ticker subscriptions follow that discovered catalog;
book subscriptions resolve it by native ID. Its order builder can sign a
synthetically injected non-perpetual market's hash/decimals, but that is latent
generic dispatch, not current catalog-backed product coverage.

The TS market snapshot retains only `ih/bd/qd`; kind, expiry, strike and
settlement semantics are discarded. Symbol cancellation explicitly filters
`PERPETUAL`, base and quote. Thus removing Rust's discovery filter and creating
synthetic option fixtures would neither reproduce a currently exposed product
nor establish a usable option lifecycle. No authenticated writes are needed to
establish this boundary, and none are authorized or performed for this audit.

## Dated public evidence

Read-only HTTPS POSTs on 2026-10-20; both environments returned successful
`r` arrays. Omitted `ia` includes inactive instruments according to the official
contract. Counts are observations, not permanent exchange constraints.

| Environment | Body | Result |
| --- | --- | --- |
| `market-data.grvt.io` | `{}` | 194 rows, all `PERPETUAL` |
| `market-data.testnet.grvt.io` | `{}` | 202 rows, all `PERPETUAL` |
| `market-data.grvt.io` | `{"k":["FUTURE","CALL","PUT"]}` | zero rows |
| `market-data.testnet.grvt.io` | `{"k":["FUTURE","CALL","PUT"]}` | zero rows |

Raw response SHA-256: production default
`d118361d9375369c398c805d9b4a76b66423d22f8a198f348b414c3538941598`;
testnet default `65d3db23982db91952c837ee2178d20d4b0543f8370d985117905ceed91ad87d`;
both explicit non-perpetual responses
`1f6931c925674dba9b7190945a6ee268271c9804dce70a73823092119ab4a106`
(`{"r":[]}`). Public captures contain no credentials.

## Plan and phase gates

1. **Public discovery/data:** independently audit the decision above. Pin the
   TS discovery body and distinguish synthetic mapper acceptance from actual
   supported products. Exercise mixed-kind Rust catalogs, subscriptions,
   ticker/book handling and refresh. Correct GRVT CLAUDE and phase-9 blocker
   wording with this explicit retirement decision, rather than inventing typed
   futures/options metadata for unexposed products. Test and sub-agent review.
2. **Private data:** audit positions/orders/fills and account settings against
   the retained product boundary; verify non-perpetual IDs cannot be aliased to
   perpetuals. Add focused regressions if a boundary is unproved. Preserve
   session, EIP-712 and stale-snapshot contracts. Test and sub-agent review.
3. **Trading:** audit preflight, cancellation and replacement for unsupported
   market identities. Prove rejection before writes for retired product IDs;
   preserve valid perpetual lifecycle and native caller IDs. Test and review.
4. **Queries:** audit symbol resolution and kind filters; prove unsupported
   product queries cannot return another market's history. Test and review.
5. **Gates/landing:** bans, fmt, strict native/Wasm Clippy, GRVT/engine tests,
   package typecheck/lint/tests, harness/host/consumer tests, combined and
   isolated Wasm as appropriate. Fresh 20-second GRVT two-book public capture
   and gate, GRVT private gate, Binance public/private regressions. Record
   numbers without threshold or oracle relaxation. Merge latest landing,
   push/PR into `feat/rust-rewrite`, post independent code-correctness reviews,
   fix/repeat, green CI merge, fresh follow-up branch.

Future enablement is a separate product port: request explicit kinds, verify
real listed ORDERBOOK instruments, preserve typed kind/expiry/strike/settlement
metadata through frames and consumers, and prove data, signing, account and
exact-instrument cancellation/replacement/history for each product. An enum,
generic signing path or perpetual replay is insufficient. `STABLE_PERP` in the
current upstream enum is likewise outside the existing TS discovery scope.

## Progress

- Discovery/source audit and public probes complete. Independent reviewer
  confirmed the default-kind contract and dormant generic paths; no port of
  Future/Call/Put is justified by current TS exposure.
- Public phase: TS discovery/injected-mapper regression **9 PASS**; Rust
  public **12 PASS** and catalog refresh **4 PASS**. Independent review found
  one missing subscription assertion; the test now captures initial sends and
  proves only the retained perpetual gets a ticker selector. A mixed-kind
  refresh also cannot enable retired products. Scope docs and the phase-9
  inventory now link this decision; TS implementations/defaults are retained.
- Private phase: independent source audit found exact native-ID/canonical-symbol
  lookup throughout positions, orders, leverage and fills. Two new account
  regressions prove same-base/quote retired IDs cannot remove perpetual orders
  or positions, change settings, emit fills or poison supported fill dedup.
  REST unknown-order/position snapshots fail atomically; leverage ignores
  undiscovered IDs. **22 account tests PASS**; independent phase review accepted.
- Trading phase: independent audit accepted the existing market preflight and
  identity rules. Mixed catalogs plus native/canonical retired IDs prove whole
  batches reject before writes, including symbol cancellation and leverage;
  unsupported IDs cannot cancel/amend a perpetual. A subsequent valid order
  still sends its exact perpetual instrument. **28 trading tests PASS**.
- Query phase: independent audit accepted bounded exact-market resolution.
  All five query shapes reject retired symbols before HTTP. Retired rows cannot
  credit perpetual fills/funding; order history, client lookup and public trades
  fail closed on an undiscovered product identity. **21 history tests PASS**.
  No production defect or TS behavior change was needed in these phases.
- Harness gates and landing evidence follow.
