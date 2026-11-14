# BreakoutProp protective edits

Historical record: this venue was retired from the SDK on 2026-11-07.
The implementation paths and validation commands below describe the former adapter.

Proven standalone STOP and closing LIMIT/TP edits use DXtrade's native modify:
GET `/accounts/{accountCode}/orders`, then PUT the complete single-order request
with the returned strong ETag in `If-Match`. The inspection must establish a
unique, matching, working and unfilled native order, with no group links or
unsupported controls. Missing provenance, attached orders and groups are refused.
The PUT preserves native side, position effect/code, quantity (including zero),
TIF, expiry and metadata. `type` is omitted as required for replace requests.
Position-linked protection cannot be converted to a fixed quantity by resizing.

Immediately before each PUT, the single venue send path checks that the Store
order is unchanged and unfilled, and that closing protection still has a positive
position on the protected side (and matching native position code when supplied).
The same check runs after rate deferral and the existing single 401 re-login.
Order writes remain legacy; host-signer latency is a managed-migration follow-up.

Reference admission uses genuine bid/ask fields received in a Quote, stamped with
local per-field receipt times. Below-market triggers (sell SL, buy TP) must be
strictly below BID; above-market triggers (buy SL, sell TP) strictly above ASK.
Equality is crossed. This conservative BBO rule refuses a crossed trigger whether
the venue evaluates last, bid or ask; synthetic Ticker last/mark/index never
provide admission evidence. The selected receipt must be finite and aged
0–5,000 ms inclusive; absent, future or older receipts refuse with `reference
stale`. Invalid/failed quote responses cannot renew receipts.

Native 404/409/412 refusals return NotSent, with no cancel fallback. HTTP 429,
408, 5xx, transport loss or an unproven successful acknowledgement return Unknown
and direct the user to Open Orders. They never claim protection was restored or
removed, and never put back or resend. A native modify does not remove the old
stop first, so no protective cancel/recreate or restoration path is introduced.
Ordinary opening LIMIT amendments retain their existing cancellation-proof flow.

Official contracts:

- [Modify Order](https://demo.dx.trade/developers/#/DXtrade-REST-API?id=modify-order)
  requires a complete conditional request, keeps order identity, and documents
  nonfinal orders and 404/409/412 refusals.
- [Single Order Request](https://demo.dx.trade/developers/rest/types/single-order-request.md)
  documents STOP/LIMIT, omitted replace `type`, TIF/expiry, and closing-position
  quantity semantics.
- [Order](https://demo.dx.trade/developers/common/types/order.md) documents native
  provenance and says group links are absent for standalone open-order rows.

Fixture changes: the two zero-close amendment controls now inspect working native
orders and assert conditional PUT, unchanged identity, zero quantity and preserved
position linkage, instead of DELETE/history/POST. Their unsafe quantity, fill,
effect, missing-order and terminal-state refusals remain. The amount-only stop
control now starts with fixed quantity 0.01, rather than a whole-position zero
order, before requesting 0.02; its documented 3,500 trigger assertion is unchanged.
Whole-position zero behavior has separate native-wire and resize-refusal controls.
