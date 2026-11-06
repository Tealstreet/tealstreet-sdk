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
silently deleting protection. Documented definitive native refusal (including code 3001 in HTTP 200/404)
is NotSent with no cancel fallback; server, unlisted and ambiguous codes remain Unknown. Native price edits share a 500 ms per-engine write floor
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

A positive quantity amendment of a full-position TP/SL order is refused before
cancel. It cannot silently convert position-wide protection to a fixed-size stop.
Price-only and zero-sentinel edits still use native modify and preserve full-position
coverage. Sized partial TP/SL edits retain their original quantity type.

A standalone stop-limit trigger move is refused before native modify or cancel:
the current edit has no independent execution-limit-price field, so retaining its
old limit would change the trigger/limit offset. Quantity-only changes preserve
both original prices. Stop-market trigger edits retain their native route.

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
Unknown, with no protection claim or restore/resend. The same applies to server
codes 500/501/9999, unlisted or fractional codes, malformed envelopes and ambiguous
per-order results. Only a single definitive row without an accepted identity can
authorize put-back; a blanket `success:false` or `errorCode` is not that proof.
A native modify may have applied; its visible error asks the user to check Open Orders, never “not changed”.
An unknown placement or
put-back outcome never restores/resends and reports status unknown, asking the
user to check Open Orders and their position before acting.

These writes remain legacy `cx.request` plus `sign_http` requests. The guard
runs before signing: there is no final-claim `http_release_veto` or
`present_plan_failure` on this path. Managed protective Plans are a separate
follow-up; this live fix does not claim to close the host-signer latency window.
No engine release-veto API, transport inventory, replay allowance, CPU threshold or combined wasm ceiling changes here.

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
  volume change; positive quantities now refuse before cancel and preserve the full-position
  row instead of asserting a conversion to partial TP/SL (`volType=1`).
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

## Live write-outcome corrections

Error classification uses the documented closed refusal set for both the outer
envelope and a single per-order result; HTTP 4xx alone is not definitive protective
write evidence. [MEXC code 3001](https://www.mexc.com/api-docs/futures/error-code)
is a trigger-order price-type refusal and remains Rejected (NotSent for native
modify), while undocumented codes remain Unknown.

Two existing controls change their contracts:

- `explicit_full_position_stop_amend_keeps_zero_sentinel_and_positive_contract_amounts`
  previously permitted converting full-position to partial protection. The renamed
  zero-sentinel control keeps native modify; the new resize regression refuses
  positive quantities before cancel for both SL and TP.
- `protective_partial_or_paired_refusal_is_unprotected_without_guessing_restore`
  previously resized a full-position pair before testing refusal. It now uses a
  sized pair, preserving the actual-cancel/refusal/Unprotected assertions; a
  full-position resize is covered by the earlier refusal regression.

The native 3001/404 refusal test is unchanged: its code is documented. New controls
reproduce the two unsafe write classifications and both recreate hazards, retain
one-shot exact restore for documented refusals, and pin malformed/mixed results and
quantity-only stop-limit preservation.
