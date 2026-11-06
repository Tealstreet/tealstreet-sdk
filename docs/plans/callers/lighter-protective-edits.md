# Lighter guarded protection edits

Lighter Core and lighter-robinhood share the guarded serial edit path. Native
ModifyOrder (transaction 17) is a separate follow-up, including its signing
security review. This change does not add that transaction or a cancel fallback.

Native stop-limit types 3/5 refuse before any cancel. Their execution price is
retained in conditional order metadata; they cannot become market stops 2/4.
A closing native LIMIT keeps its LIMIT type when repriced across the market.
Sparse ambiguous updates may retain prior display metadata, but cannot grant
native restoration provenance.

An edit of SL/TP or a closing LIMIT requires a complete accepted standalone,
unfilled native row. The native class, side, quantity, execution and trigger
prices, time in force, expiry, reduce-only flag and explicit absent group links
are captured independently from the requested edit. Missing fields, group links,
attached/split shapes and unsupported native classes refuse before cancellation.
The capture is pruned with the current accepted order census.

Only a genuine accepted MARK field grants a local receipt. REST rows retain their
actual item receipt until successful envelope completion; sparse rows and
index-substituted marks do not refresh it. A rounded SL/TP trigger must remain on
the non-triggering side of MARK, with equality crossed and age at most 5000 ms.
At every managed final Claim, the pure venue veto rechecks that evidence, the
unchanged unfilled source and the protected position side for closing orders.
Nonce-21104 retry, re-signing and deferred releases use that same veto. No Store
write, I/O or await occurs in the guard. Ordinary explicit cancels are unchanged.

An accepted cancel transaction is not terminal-order proof. The serial Plan next
reads authenticated `accountOrders` by the original client index, requiring the
exact native target, market, client index, canceled status and zero execution.
Empty, foreign, filled, duplicated or unproven results withhold the replacement.
The edit alone declares the read credential needed by this managed continuation.
The accepted cancel write fact is not rewritten when a later proof read fails.

A definitive replacement refusal permits one put-back of the captured ORIGINAL
size and native parameters, with the original absolute expiry. A fresh generated
client index identifies the put-back; no native identity reuse is assumed. It
passes the final reference/position guard again. A targeted subsequent read must
prove the exact open original parameters before reporting typed `Restored`.
Definitive failure or unavailable restoration authority reports `Unprotected`
and asks the user to re-place protection. The current leg's presentation is set
before dispatch and terminal failure using the existing Plan presentation hook.

Guarded HTTP 405/429, native rate codes 23000/23100, 408/5xx, transport loss and
ambiguous responses are unknown: no put-back, resend or protection claim. A
put-back does not get even the normal single fresh-nonce retry. Unknown reads
retain explicit “check Open Orders” wording; the engine continues to own the
original write fact and aggregate command outcome. Unguarded classifications
and their existing controls are unchanged.

VENUE_STATE remains the same 11-byte version 1 (account index and tier). Native
provenance and mark receipts are not exported as fake frame-local evidence.
Frame-fed protective price edits refuse `reference stale`; ordinary orders,
explicit cancels and ordinary LIMIT edits retain their existing behavior. A
future mapped-receipt seam must establish freshness before enabling those edits.

Native endpoint contract: the official Lighter Python SDK documents
[`accountOrders`](https://github.com/elliottech/lighter-python/blob/38a34672e7cb43c67d807ece25f125fda2dfdecf/docs/OrderApi.md#account_orders)
as lookup by client indices, covering recent active/inactive orders. The
[order model](https://github.com/elliottech/lighter-python/blob/38a34672e7cb43c67d807ece25f125fda2dfdecf/docs/Order.md)
names the native size, type, expiry, trigger status and group-link fields.
[Trigger conditions](https://docs.lighter.xyz/trading/order-types-and-matching)
use MARK. No live or demo orders are required by these controls.
