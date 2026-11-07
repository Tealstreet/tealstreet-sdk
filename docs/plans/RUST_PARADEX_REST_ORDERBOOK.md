# Paradex REST order-book parity

Fresh actual landing: `d510b9813235f7bf99c6eac9c630a583fb0c1212`.
Plan first; independent of other venue implementations. Only the precise shared
book decoder from reviewed Backpack #1680 is a genuine prerequisite; integrate
its exact reviewed head and require actual prerequisite landing before merge.

1. Reproduce missing native capability/JSON admission/query. Match TS unsigned
   GET /orderbook/{encoded native symbol}?depth=N, default20 and explicit caller
   depth forwarded unchanged, on live/testnet. No market seed or credentials.
   [Official endpoint](https://docs.paradex.trade/api/prod/markets/get-orderbook)
   confirms default20 and supplies no allowed-depth list; do not invent a clamp.
2. Add a separate unstreamed command-owned continuation, never Pending::Public,
   discovery/account startup, JWT/signing, public books or stream sequence state.
3. Require actual object plus bids/asks arrays, permitting empty/one-sided books.
   Parse first two tuple fields and ignore metadata; preserve native order,
   duplicates/zero quantities and per-level total=price*baseAmount. No IDs,
   cumulative totals, sorting, scaling or lastUpdateId. Fail malformed/nonfinite
   finances and overflow atomically. Metadata/version fields remain irrelevant.
4. Public HTTP/native/transport errors stay typed owned NotSent, retaining only
   structured venue reasons/code/message and bounded usable rate hints. HTTP
   status has precedence; auth-looking public errors cannot invalidate accounts.
   No automatic retry or stream recovery.
5. Enable precise standalone decoder and native capability, update combined-host
   and SDK expectations/bridge test. Regress exact URLs/limits/noseed/noauth,
   values/order/zero/tuple metadata, empty/malformed structures/finances, native
   and HTTP errors/rate hints, concurrent owners and duplicate callback, and live
   subscribed book/ticker/private account independence with next WS continuity.
6. Independent plan/implementation reviews; Paradex+Binance focused native gates,
   default combined-host capability test, SDK test/typecheck/lint and strict
   Clippy/fmt/bans. Full exact-head CI, fresh separate scope guard, actual merge
   verification and overseer/caller receipts. Preserve current KCS build rollback.

No new products, NX edits, TS deletion, budgets/replay allowances, local release
WASM/full replay/timing matrices or release automation. Four local Cargo jobs.


Implementation evidence:
- Eight original-source groups failed through standalone JSON and actual venue
  calls, then pass. They pin encoded live/testnet GET, default/explicit depth,
  native order/zero quantities/per-level quote totals, empty sides, typed native/
  HTTP/read failures, bounded rate hints, malformed/overflow atomic rejection
  and concurrent exactly-once ownership. Extra tuple fields are ignored as TS.
- An actual SBE snapshot survives REST success and public401 unchanged alongside
  ticker/account state, and the next native delta still applies. A private-mode
  regression proves a public auth-looking refusal does not refresh/invalidate
  the JWT used by a subsequent successful private account-identity query.
- Existing unsupported-command regression now excludes this supported command
  while retaining all other refusal assertions. No private trading production
  or live stream decoder code changed.
- Final local checks pass:432 Paradex+Binance native tests, default combined-host
  capability test,22 SDK bridge tests, typecheck, explicit no-ignore ESLint and
  strict venue Clippy/format/bans. Two independent reviews CLEAR; full exact-head
  CI and current-landing integration/separate guard remain required before merge.
