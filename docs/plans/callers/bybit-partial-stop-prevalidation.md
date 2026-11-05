# Bybit partial-stop cancel admission

A partial position stop still uses Bybit's existing cancel/recreate flow. The
position trading-stop API adds Partial legs; it does not amend an existing leg.
Before planning a destructive cancel, require a fresh price for the reference
that the replacement body will actually send. Repeat that check inside the
managed request's final synchronous claim, after preparation/signing and pacing.

The guard captures the planned symbol, side, replacement kind, raw trigger and
reference. It reads current Store evidence at the final cut: at most 5 s old,
positive and finite, with no future observation accepted after clock rollback.
A missing/stale reference or an immediately firing trigger refuses with typed
InvalidInput/NotSent and an unchanged-stop explanation. It never borrows another
reference to admit the cancel. The old stop remains open, the plan/claim retires,
and another valid amend can proceed normally.

`Venue::http_release_veto` is a pure read-only Store check on a single-request
claim. It has no I/O, scheduler/custody mutation or retained snapshot; its default
returns Allow. Only Bybit overrides it, and only Partial-stop cancellation uses
it. An error goes through ordinary pre-send failure cleanup, including pending
continuation, reconciliation, signing and nonce custody. No write fact changes
after release. Existing atomic group admission and nonce scheduling are unchanged.

A price can still move after the release cut. The existing native-refusal and
exact-old-stop restoration path remains intact for that race. No new retry,
refresh or position-setting behavior is introduced. Missing-mark non-destructive
fallbacks and in-place standalone/full/attached amendments are unchanged.

Controls cover the reported long/LastPrice shape, missing/stale references, a
price move or aging while genuine preparation waits, the inclusive 5 s boundary,
and a successful amend after refusal. Engine controls pin current-Store veto,
no-op request order/wire/release clock, claim cleanup and nonce floor preservation.
Existing DK2 restore/concurrency fixtures now supply a safe fresh reference at
cancel release; the venue-refusal control moves Last after the cancel. Their
wire and outcome assertions are unchanged.
