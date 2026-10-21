# Crypto.com REST order-book parity

Fresh landing: `3cd168d2c0879d3cd776c3e59583f4994d690691`.
Short plan first; this venue is independent of MEXC runtime changes. The precise
shared decoder feature from reviewed Backpack #1680 is a genuine prerequisite;
prefer its actual merge before publication, or pin/review the exact dependency.

1. Reproduce missing native capability, native query and standalone JSON ingress.
   Verify TS's interceptor: fetchOrderBook's apparent POST becomes unsigned GET
   /public/get-book with raw instrument_name and fixed depth50, live/UAT root.
2. Add a separate request-owned unstreamed continuation and typed REST decoder,
   independent of stream Book/cache/startup/catalogue/account/history pacing.
   Use required numeric REST code; preserve native refusal before empty fallback.
3. Match TS first-book-only mapping: raw price string IDs, parsed price/quantity,
   native order, zeros retained, total0, no contract scaling/update ID; ignore
   caller limit. Native code0 empty data is an empty book, while malformed or
   nonfinite financial levels reject atomically. Explicitly pin missing/null
   envelope/first-book behavior against the actual TS unwrap rather than a WS
   decoder that defaults missing code to0. Ignore later book rows as TS does.
4. Enable Crypto.com's precise order-book decoder and capability, align shared
   host/SDK fixtures and bridge regression. Keep all other venue bits unchanged.
   HTTP/native/transport failures stay owned, typed and non-auth for public data,
   retaining native diagnostics/usable cooldown hints without automatic retries.
5. Regress live/UAT encoded GET/fixeddepth/noauth/noseed, standalone JSON, exact
   lexical IDs/order/zero totals, empty and malformed envelopes/levels, later
   irrelevant data, concurrent owners, failures and live-book independence.
6. Independent reviews, Crypto.com+Binance native gates, shared host capability
   test, focused SDK checks, strict Clippy/fmt/bans, full exact-head CI and fresh
   separate scope guard before merge/report. No budgets or local release builds.

MEXC #1681 is ordered immediately after Backpack; coordinate shared fixture
integration before this independent family merges. GRVT builder errors and
browser evidence remain codex-uufub4's scope. No NX writes or TS deletion.
