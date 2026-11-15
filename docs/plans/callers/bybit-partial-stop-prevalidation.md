# Bybit Partial-stop protective edits

Partial `/v5/position/trading-stop` is add-only. An edit never falls back to
that endpoint without first cancelling and proving removal of the old leg.
Missing or stale genuine Mark evidence refuses NotSent before any write. Kind
projection keeps the existing raw drag-price contract; new SL placement retains
Last when requested and Mark otherwise, including an Index placement request.
Edits and exact put-back preserve the native Last/Mark/Index source.

Partial edits alone use a serial engine Plan. The initial veto requires an open,
unchanged and unfilled original, a position on the protected side, and genuine
per-field receipts no older than 5 s for both the original and replacement
sources. Future observations and equality at the trigger boundary refuse. The
same checks run at the final physical cancel claim, including a re-signed cancel.
Every replacement and put-back claim rechecks the captured economics, zero fill,
protected-side position and the source its body actually sends. After a proven
cancel, a failed release check reports Unprotected and asks the user to re-place.

A cancel ACK does not publish Canceled or authorize creation. A new managed read
filters `/v5/order/realtime` by the exact orderId and symbol. Only a native
Cancelled or Deactivated row with matching category, side, positionIdx, Partial
kind, trigger, execution type/price, original native quantity and zero fill proves
removal, including when the target reappears in a peer census. Partial mode
accepts StopLoss/PartialStopLoss for the original SL and
TakeProfit/PartialTakeProfit for the original TP in both removal and standalone
put-back proof; opposite, unknown or missing kinds never match.
A still-open or absent row gets at most three realtime reads, 100 ms apart,
then one targeted history read (closed realtime rows can disappear on a venue
restart). Missing, filled, changed, malformed or wrong-scope proof never creates
anything. The local cancellation/replica fence is installed only after proof.

`standalone_provenance` is the single put-back eligibility predicate. It requires
an explicit empty parentOrderLinkId, a native Partial row with its own orderId,
exact original quantity and execution terms, a nonempty creation link and a
valid creation time. Complete bounded live and history peer censuses must show
no sibling sharing that link, parent link or creation time on the same side.
Any other same-side Partial peer within the ±5 s window also prevents put-back,
even with a different creation time and unrelated link.
The History request is bound when issued to the proved target's native
createdTime ±5 s (with a positive lower bound), never Store last_update or an
account-wide seven-day default. Missing, malformed, zero or future native
creation time skips History and disables put-back, while allowing the guarded
replacement after terminal proof. A definite replacement refusal then reports
Unprotected and asks the user to re-place; no lineage is guessed. A nonempty cursor or missing/out-of-window returned
creation time never grants restoration authority.
Missing lineage, an incomplete census, attached/grouped/split shapes, or an
opposite leg whose binding was lost grants no restoration authority. Creation
must be within the 24 h retention window for unfilled cancelled history; older
lineage cannot be proved by an empty history page.

A definite replacement refusal permits at most one put-back of a positively
proven standalone original. Its size is never rounded to current market
precision; trigger source, Partial mode, side and Market/Limit execution are
kept exactly. A positive native setting ACK reports typed Restored. A definite
put-back refusal, or unproven standalone lineage, reports typed Unprotected with
re-place wording. The original already-gone cancel classification never grants
restoration authority, even if its follow-up read proves a cancelled row.

Replacement and put-back have one physical write each. Unknown HTTP outcomes
(including 429, 408 and 5xx), native rate limits, unlisted codes, ambiguous
not-modified replies, malformed bodies and transport loss never restore or
resend. The protective classifier is derived from Bybit's ordinary table;
ordinary writes and other setting calls keep their existing policy. Plan failure
presentation supplies Open Orders wording even when the engine seals a request
without a venue callback. It is armed before the initial Partial cancel too:
released transport loss and timeout keep typed Unknown, while definite native
refusals and known unsent preparation failures retain their original cause and
outcome. Reads use Read authority and grant no write facts.

Controls cover all five derivative profiles, absent/live/filled/mismatched native
proof, delayed completion, lost proof transport, standalone/attached/lost-binding
lineage, unchanged/filled/position vetoes at delayed claims and re-sign, exact
original size and limit execution, every native source, Unknown replacement and
put-back responses, and existing concurrency, replica and full/attached controls.

API references: [trading-stop](https://bybit-exchange.github.io/docs/v5/position/trading-stop),
[realtime orders](https://bybit-exchange.github.io/docs/v5/order/open-order),
[history retention](https://bybit-exchange.github.io/docs/v5/order/order-list).
