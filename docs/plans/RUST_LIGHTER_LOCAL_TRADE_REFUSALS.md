# Lighter local trading refusal custody

Owner claude-55ao7t, plan first on `feat/rust-rewrite` at `ba6fb191239`, branch
`feat/rust-lighter-local-trade-refusals`. The Aster #2027 pattern, applied to
Lighter's synchronous `trade_command` refusals. Reviewer dazzy3.

## Scope

`trade_command` (`venues/lighter/src/trading.rs`) is reached only from the
genuine `on_command` entry. Ten of its branches refuse with `cx.fail` before any
queue, nonce, signing or I/O:

| Command | Refusal |
| --- | --- |
| PlaceOrders | a caller client order ID; a placement plan error |
| CancelOrders | a non-numeric order index; an unknown order; a cancel plan error |
| AmendOrder | an amendment already pending; an unknown order; an order not open; an amend plan error |
| SetPositionMode | hedged mode (Unsupported) |

Each becomes one genuine Mutation admission (`admit_http_command`, no
credential, no pacing, `RetryPolicy::Never`) settled by `fail_context` with the
same native error. Unchanged: SetPositionMode's one-way success, which stays
on its legacy `cx.done`; valid jobs; the nonce queue; every async terminal.
No hosted signing, signer or nonce code is touched.

## Caller-visible contract

With free capacity, every refusal keeps its exact native message, kind and
`NotSent`, with no I/O. Full genuine owner capacity or an invalid clock now
yields the engine's one typed Mutation admission refusal (`NotSent`) instead of
the native text, as for every adopted venue. Recorded in RUST_CUTOVER_CALLERS.

## REDs and controls

Real Engine, ready private fixture, 256 genuine managed candle owners for
capacity (the `request_context_cached_reads.rs` pattern). Six refusals are
reachable from a ready engine: caller client ID, placement plan error,
non-numeric cancel, unknown cancel, amending an unknown order, hedged mode.
The other four share the same helper and need seeded order state.

- Controls (pass before and after): with free capacity each refusal gives one
  `NotSent` terminal with its native message and no HTTP.
- REDs: at full capacity each gives the typed admission refusal, not the
  native text. Today they return the native text.

## Inventory and size

`venues/lighter/src/trading.rs` goes 15 fingerprints / 17 occurrences to
7 / 7: the ten refusals, shrink-only; no other path changes. Local fresh builds on
`ba6fb191239`: Lighter 1,031,246 to 1,031,343 (+97) against 1,343,488;
combined 13,894,756 to 13,894,837 (+81) against 14,155,776. CI gives the
authoritative numbers.
