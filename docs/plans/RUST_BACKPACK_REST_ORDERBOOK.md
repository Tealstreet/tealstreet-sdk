# Backpack REST order-book parity

Fresh landing: `421e523a3bfdfa99c70c072b01ced9cf2f53f267`.
Assigned API scope also includes Crypto.com, GRVT, Paradex, WEEX books and
Hyperliquid scheduleCancel, each on its own fresh branch/PR. codex-uufub4 owns
WEEX metadata, GRVT builder errors and browser measurements; avoid that overlap.

1. Reproduce Backpack's unsupported FetchOrderBook and false native capability.
   Pin TS public GET /api/v1/depth?symbol=..., unmodified native side order,
   raw price IDs, parsed price/amount and total0; TS ignores an optional limit.
2. Add a request-owned typed public REST continuation, independent of stream
   subscription/snapshot/generation state. Preserve raw price identity and reject
   malformed/nonfinite financial data rather than return corrupt numbers.
   Empty sides remain valid; no credentials, market seed or stream is needed.
3. Enable only Backpack's existing orderBookQuery bit and update the combined
   host capability fixture. Use existing command/wire/SDK/worker plumbing; no
   new command schema, write semantics, stream mutation or retry policy.
4. Native regressions cover path/encoding/ignored limit, exact values and IDs,
   empty/one-sided/malformed envelopes, concurrent ownership, transport/native
   errors and independence from streaming state. Cover the existing SDK bridge.
5. Independent plan and implementation reviews; Backpack+Binance native checks,
   shared host capability test, strict Clippy/fmt/bans and focused SDK checks.
   Push full CI; fresh separate exact guard before merging/reporting actual pin.

Other venues remain refused until their own parity batch lands. Preserve retired
Backpack spot and all cutover owner/live/NX boundaries; no budget changes or local
release WASM/full timing matrices. Four local Cargo jobs.

Plan audit refinement before implementation:
- Actual WEEX metadata #1675 (`22c6c6133f2841f8cce897666a44c214f6f79bba`)
  is genuinely integrated before source work, preserving its shared capability
  fixture/SDK changes and its independent metadata policy.
- Existing JSON FetchOrderBook decoding is cfg-gated only by AsterDEX/Polymarket
  command features. Add a precise command-order-book feature, retain their
  activation through feature dependencies, and enable it for Backpack via its
  engine dependency. Add a real JSON-ingress regression in the single-venue
  native build; combined host and direct enum tests alone hide this defect.
- MEXC appears to share the pre-existing standalone decoder omission; report it
  separately and keep its venue activation out of the Backpack behavior batch.
- Keep zero quantities, raw lexical price IDs and unsorted/duplicate levels as
  TS does. Public errors must remain typed and owned, never account auth.
