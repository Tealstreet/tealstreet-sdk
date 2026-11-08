# BloFin socket liveness

Both feeds use the engine-managed lifecycle with independent epochs and retries.
Public keeps connect 20 s, idle 20 s, ping 5 s and backoff cap 16 s. Private keeps a 10 s
connection deadline, separate 10 s authentication deadline, ping 5 s and idle 20 s.
Pongs refresh liveness but never extend authentication. Only definitive code 0
login proof enables subscriptions and private financial payload publication.

The unchanged Primary SessionAuth provider signs a fresh login per current
Opened. The signed login is never an interest or replayed on reconnect. Public
interests preserve refcounts and books5 sharing; sequence gaps request managed
repair. Retirement invalidates books before replacement rows. See
`docs/plans/RUST_BLOFIN_MANAGED_SOCKETS.md` and production-binding controls in
`tests/managed_sockets.rs`. Replay tapes are scripted evidence, not live account
connectivity evidence.

Dispatched writes answered by HTTP 429, 408 or 5xx remain unknown through the
engine journal, regardless of a native refusal body. Native code `429` inside
HTTP 200 is also unknown, including a row with order ID `0` under an
`All operations failed` envelope: it cannot prove that nothing was placed or
authorize a protective put-back. Native rate failures direct the caller to
Check Open Orders and retain the venue code/message as diagnostic evidence.
Reads and documented definitive refusal codes on other statuses are unchanged.

Trading validation and compensating writes follow the legacy TS adapter.
Normal split lots and grouped rows do not gain a whole-plan native min/max gate;
legacy partial-placement cases remain known issues. Existing zero-size and
opening-trigger shape/provenance checks remain in the venue driver. Unknown
stored protection references normalize to the TS default `last`.

`WritePlan` reports per-element outcomes without selecting execution policy.
The existing native amendment fast path retains its cancel/re-place fallback
for a surviving TP/SL sibling after a definite refusal or failed verification.
Fallback steps are registered in the same journal, while unused branches remain
notSent. TS has no original-order put-back after a refused replacement, so that
separate Rust behavior is absent. Accepted cancels, settings and amendments use
`Applied` with known target references. Cancel targets never become created IDs;
engine classification, reconciliation fences, custody and signing remain intact.

After-failure dispatch follows the legacy TS envelope/message rules: normal
batches stop on a non-success row message; TP/SL children and normal/TP-SL
cancels continue after definite row refusals under envelope `0`. Trigger row
failures and failing envelopes stop. `WritePlan::record` only reports outcomes;
it neither validates payloads nor grants or refuses dispatch.

Known boundary: the shared serial engine still prevents continuation after an
Unknown native outcome, even where TS ignores that row. The ignored red probe
`code_only_unknown_row_needs_engine_continuation` records this unresolved parity
gap; changing the engine policy requires the lead's decision.
