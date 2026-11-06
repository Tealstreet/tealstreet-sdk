# MEXC protective edits

Protective edits first inspect the original native order. The read is bounded to
20 seconds and 20 pages of 100 rows; absence, an ambiguous identity, an unknown
class or a changed/filled order does not permit cancellation.

## Native price edits

For a price-only change with verified native provenance, use exactly the class
MEXC documents:

| Class | POST endpoint |
| --- | --- |
| Untriggered plan trigger | `/api/v1/private/planorder/change_price` |
| TP/SL planned-order ID | `/api/v1/private/stoporder/change_plan_price` |
| Attached TP/SL on a limit parent | `/api/v1/private/stoporder/change_price` |
| Attached TP/SL on a plan parent | `/api/v1/private/planorder/change_stop_order` |

The attached-leg aliases resolve to their native parent. Both TP and SL prices
and their independent trigger references are retained, including the unchanged
peer. In particular, the plan-parent endpoint interprets an omitted/zero price
as a removal, so a missing peer's provenance refuses the edit rather than
silently deleting protection. Native refusal (including 404) is NotSent with no
cancel fallback. Native price edits share a 500 ms per-engine write floor
with the existing submission pacing. An unproven response remains Unknown. A newer stream update
wins over the optimistic local native-edit acknowledgement.

## Guarded quantity / other edits

A protective edit requiring replacement runs the veto in `queue_trade`, the
single venue-side signed-send path. Deferred sends pass through it again. The
veto checks the original's complete local row, zero fills, a genuine receipt
of each replacement trigger's own Last/Mark/Index source no older than 5 seconds
(inclusive), strict uncrossed direction for the rounded trigger actually sent and an open protected-side
position for a closing stop. A missing, zero, disconnected, future or stale
reference refuses with “reference stale”; Last-substituted prices and sparse
updates do not count. HTTP ticker receipts commit only after successful delivery,
and retain their original per-row receipt time.

The cancel acknowledgement alone does not permit replacement. Plan and TP/SL
orders now have an explicit terminal-state read, just as ordinary limit/market
amendments already do. The exact order must be canceled, not triggered/filled,
and its native economics and execution parameters must match. Reads are bounded;
an absent row does not prove cancellation. A veto clears the FIFO job and allows
a later valid edit.

A sized amendment of a full-position TP/SL order becomes a partial TP/SL order
(`volType=1`) with the requested native contract count, preserving its native
execution parameters. A price-only/full-zero-sentinel edit uses native modify
without resizing the position's stop. The caller must omit `amount` (or send 0
for full-position stops) for price-only edits; an explicit positive amount
requests fixed-size protection. Web price drags send only `{price}` in `update`;
the Rust adapter forwards only fields present there, not `order.amount`.

After a definitive replacement refusal, a standalone stop with complete native
provenance gets one put-back of its original quantity, side/direction, trigger,
reference, execution type/price, protection flags, mode and TP/SL parameters.
The original is rechecked against the current reference and position before that
send. Its venue-assigned ID changes; these plan/position-stop placement APIs have
no caller-ID parameter to reuse. Success reports typed Restored. Refused or
withheld put-back reports typed Unprotected with a visible “re-place” message;
there is no loop and no immediate market fallback. Partial, attached and grouped
orders never borrow standalone restore authority. HTTP 429 or typed RateLimited
(including HTTP 200/code 510) on replacement, put-back or native modify is
Unknown, with no protection claim or restore/resend. A native modify may have
applied; its visible error asks the user to check Open Orders, never “not changed”.
An unknown placement or
put-back outcome never restores/resends and reports status unknown, asking the
user to check Open Orders and their position before acting.

These writes remain legacy signed requests, as approved. The guard runs at the
last venue-side send admission; the host-signer latency window remains a managed
migration follow-up. No engine release-veto API, transport inventory, replay
allowance, CPU threshold or combined wasm ceiling changes here.

## Native controls and fixture changes

Native modify goes through the same final source-freshness, crossed-trigger,
unchanged/unfilled-order and closing-position guard, including deferred sends.
An opening plan parent needs no position before it opens; its attached bracket
still uses the direction opposite that parent.

The native controls cover the four routes; peer preservation; definitive refusal
and 404 without fallback; both trigger directions and all three references;
5-second equality; sparse/zero/disconnected receipts; changed/filled/absent orders
and positions; a deferred cancel's final recheck; terminal-read authority and
bounded retries; claim release; one-shot exact put-back; failed/withheld put-back;
unknown outcomes without writes; and partial/group refusal without guessed restore.

Existing fixture changes:

- `amend_retains_attached_bracket_prices_and_independent_sources_across_lifecycles`:
  add the genuine native pre-cancel inspection. Its original replacement payload
  assertions for both prices and references remain.
- `plan_stop_amend_keeps_latest_native_source_after_ws`: replace the assumed cancel
  acknowledgement with a native inspection, then assert native modify and the
  original latest-source payload assertion. Its closed-long stop now has a real
  long position in the account snapshot, required by final native admission.
- `amend_full_position_stop` callers: enrich the native inspection with complete
  TP/SL provenance. Price-only/zero-sentinel cases assert native modify with no
  volume change; positive quantity cases assert guarded cancellation, authoritative
  terminal read and the exact partial TP/SL payload (`volType=1`).
- `reloaded_full_position_stop_amend_refuses_until_position_data_arrives`: retain
  every original no-position refusal assertion; after positions arrive, assert
  inspection and native price modify rather than destructive cancel/recreate.
- `protective_native_endpoints_preserve_peer_prices_and_sources`: the plan-parent
  fixture is an opening Buy (side 1, not reduce-only), consistent with its
  long-side attached SL/TP. Prices, sources, native path and peer assertions stay.
- Test support optionally exposes the real public socket for receipt controls;
  the existing helpers keep their prior signatures and behavior.

## Primary API evidence

- [Modify plan order](https://www.mexc.com/api-docs/futures/account-and-trading-endpoints/modify-plan-order)
- [Modify TP/SL planned order](https://www.mexc.com/api-docs/futures/account-and-trading-endpoints/modify-tpsl-prices-on-a-tpsl-planned-order)
- [Modify attached limit TP/SL](https://www.mexc.com/api-docs/futures/account-and-trading-endpoints/modify-tpsl-prices-on-a-limit-order)
- [Modify attached plan TP/SL](https://www.mexc.com/api-docs/futures/account-and-trading-endpoints/modify-take-profitstop-loss-on-plan-order)
- [Plan order terminal states](https://www.mexc.com/api-docs/futures/account-and-trading-endpoints/get-plan-order-list)
- [TP/SL terminal states and native provenance](https://www.mexc.com/api-docs/futures/account-and-trading-endpoints/get-take-profitstop-loss-order-list)
- [Place plan order](https://www.mexc.com/api-docs/futures/account-and-trading-endpoints/place-plan-order)
- [Place TP/SL by position](https://www.mexc.com/api-docs/futures/account-and-trading-endpoints/place-tpsl-order-by-position)
