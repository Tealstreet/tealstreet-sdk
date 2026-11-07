# MEXC parity follow-up

Branch: `feat/rust-mexc-parity`, based on `origin/feat/rust-rewrite`.
Follow `VENUE_PORT_BRIEF.md` and `RUST_REWRITE.md`; land into
`feat/rust-rewrite`, then start WOO on `feat/rust-woo`.

## Phases

1. Audit TS public/private/trading/query capabilities against the original
   adapter as well as the post-port reference. Independent sub-agent audit.
   Public and account data need no changes for these three questions.
2. Add regression coverage for supported behavior or unsupported preflight,
   including private replay cases; document the evidence. Review and commit.
3. Run native, wasm, MEXC and Binance replay gates plus repository checks.
   Open the PR, post adversarial reviews, fix findings, wait for green CI,
   merge, and create `feat/rust-woo` from the landing branch.

## Initial audit

- Opening conditional: not supported. Before the port, `placeOrder` routed
  only StopLoss/TakeProfit to `planorder/place/v2`; Conditional fell through
  to `order/submit` with type 1, without `triggerPrice`. That placed a regular
  limit, not an opening conditional. The original MEXC CLAUDE.md explicitly
  says opening trigger orders are not implemented. The post-port reference
  rejects this unsafe fallthrough explicitly.
- Trailing stop: not supported. The original `placeOrder` explicitly throws
  `MEXC trailing-stop order placement is not implemented yet`.
- Income: no TS MEXC override for `fetchIncomeHistory` or
  `fetchIncomeHistoryRange`; both inherit branded empty BaseExchange
  fallbacks. MEXC TS endpoint inventory has no income/funding ledger route.
  Rust explicit FUNDING_FEE support is additional functionality; it must not
  be mislabeled as a complete general income ledger.

Original behavior was inspected at `4137ce151f^`, before the Rust lane's TS
repairs. Current source alone would not establish the original capabilities.

## Progress

- Independent source/history audit PASS: baseline `ff0277bacd` confirms all
  three conclusions above. No new production capability is required for TS
  parity. Existing sized/full-position TP/SL remain supported.
- Added native single/mixed-batch conditional/trailing preflight tests and
  general-income no-HTTP tests, plus two mixed-batch private replay cases.
  Independent phase review PASS; 22 trading + 8 history tests pass.
- Native MEXC total: 53 tests pass; bans, formatting, native and wasm clippy
  pass.
- Fixed a fixture timing defect exposed by the extra rejected commands:
  post-cancel REST reads now reflect empty open-order lists, and private pongs
  through six seconds allow both polling loops to reconcile terminal rows.
  Three private runs PASS (25 frames), exact comparisons unchanged.
- MEXC public PASS: 14,701 frames / 1,135 markets / two books;
  CPU 2085.2 -> 900.0 ms; steady CPU 1432.0 -> 559.8 ms;
  retained memory 10.1 -> 3.5 MB. Binance public and private PASS.
- Typecheck 44 tasks, lint 28 tasks, focused TS MEXC 131 tests, harness 40
  tests and wasm shell 19 tests PASS. Wasm 2,416,535 bytes under the unchanged
  2,621,440-byte budget. Production Rust is unchanged in this follow-up.
- PR/adversarial review and green CI pending; WOO starts after merge.
