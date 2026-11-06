# GRVT protective edits

Stop/TP edits retain authoritative order inspection, cancellation acknowledgement
and the final `CANCELLED` read before replacement. The amend's actual `Cancel`
effect now shares a veto in `send_request`, including code-1000 session-refresh
replay. No HTTP ownership migration or native bulk-orders path is introduced.
Legacy host-signer latency remains a managed-migration follow-up.

Before every destructive send, the original must still exist, be open/canceling,
unfilled and otherwise identical to the inspected local snapshot, without new
execution/lifecycle progress. Closing protection needs a positive position on
the protected side. The replacement's serialized trigger source selects a genuine
positive Last, Mark or Index receipt no older than 5 seconds; trigger equality
counts as crossed. Sparse fields do not renew absent fields, fallback prices
never create receipts, and disconnect clears receipts. A veto unwinds the
command normally and leaves the original protection open.

After a definitive replacement refusal and a confirmed canceled original, one
exact put-back is allowed only with standalone provenance: a matching native
identity from a locally created standalone trigger, or a native position-linked
trigger (`close_position=true`, which cannot have order linkage). A local bracket
child is never restored independently. Exact native side, size, execution type
and limit price, TIF/post-only, reduce-only, trigger source/price, close-position,
builder and fee must match the generated restore. Unknown fields or unsupported
metadata refuse reconstruction. The old client identity is reused after native-ID
cancellation; nonce/signature and create time are fresh. The existing lifecycle
tracker fences old identity frames.

A proven put-back returns typed `Restored`; a definitive refusal or missing
reconstruction proof returns typed `Unprotected` and asks the user to re-place
protection. Put-back refusals, including an expired session, never retry.
An unknown replacement/put-back outcome never causes a further create and keeps
reconciliation advice. Existing rejected split-trigger edits stay refused before
any cancel; partial/group/attached edits are never guessed back.

Official API evidence:

- https://api-docs.grvt.io/schemas/api_bulk_orders_request/ documents TP/SL bulk
  cancel-replace with new IDs, without establishing old-stop preservation when
  the new order is refused. This change does not treat it as atomic modify.
- https://api-docs.grvt.io/trading_api/#bulk-orders lists error 2232 for
  position-linked TP/SL with order linkage.
- https://api-docs.grvt.io/schemas/api_cancel_order_request/ says native-ID
  cancellation does not install the client-ID cancellation TTL.

The existing `a_stop_whose_replacement_is_refused_says_the_position_is_unprotected`
fixture previously used a partially-filled stop without a price receipt or
position. It now uses an unfilled stop, its position and a genuine fresh Mark
receipt; its original unprotected-message assertion is unchanged. Its response
lacks exact restoration fields, retaining the unavailable-provenance branch.
A separate control refuses filled or changed targets at the shared send boundary.
