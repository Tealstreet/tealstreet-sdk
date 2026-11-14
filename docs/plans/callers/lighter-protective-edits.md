# Lighter guarded protection edits

Lighter Core and lighter-robinhood share the guarded serial edit path. Proven
untriggered standalone SL/TP edits use native ModifyOrder (transaction 17), with
no cancel fallback. The remaining eligible shapes use the guarded replacement.

Native stop-limit types 3/5 preserve their captured execution price and native
type on tx17. Otherwise they refuse before cancel; they cannot become market
stops 2/4.

A closing native LIMIT keeps its LIMIT type when repriced across the market.
Sparse ambiguous updates may retain prior display metadata, but cannot grant
native restoration provenance.

An edit of SL/TP or a closing LIMIT requires a complete accepted standalone,
unfilled native row. The native class, side, quantity, execution and trigger
prices, time in force, expiry, reduce-only flag and explicit absent group links
are captured independently from the requested edit. Missing fields, group links,
attached/split shapes and unsupported native classes refuse before cancellation.
The capture is pruned with the current accepted order census.

Source capture and put-back proof accept the documented hyphenated native
stop-loss/take-profit types alongside their existing numeric/underscore forms.
The native time-in-force names `immediate-or-cancel`, `good-till-time` and
`post-only` retain exactly the corresponding numeric values 0, 1 and 2.
Unknown types or time-in-force values remain unproven.

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

Terminal cancellation includes only the reason-qualified cancellations listed
in the [native order enum](https://github.com/elliottech/lighter-python/blob/38a34672e7cb43c67d807ece25f125fda2dfdecf/lighter/models/order.py#L69-L85),
alongside `canceled`/`cancelled`; an arbitrary cancellation prefix is insufficient.
These spellings do not relax original identity, zero fills, standalone lineage,
kind, trigger, position or release checks and do not enable grouped edits.

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

After an ordinary single write's permitted nonce-21104 retry, a native
23000/23100 response presents its unknown/Open Orders guidance instead of the
earlier nonce refusal. The journal still projects the outcome and retains the
write evidence; presentation cannot authorize another resend or put-back.

A missing, stale or Store-mismatched MARK receipt now obtains one unsigned
public exchangeStats read before any destructive admission. It retains the
original preflight owner/generation and immutable native edit, with one attempt,
a 20-second bound and no retry or source fallback. The strict target-market
Stats MARK field must be finite and positive; its actual row receipt must still
be within the unchanged five-second limit when the whole code-200 envelope is
accepted. Empty, sparse, time-only, duplicate, failed, foreign and delayed reads
refuse before cancellation. A newer genuine native MARK is preferred. The pure
reference check and final-send veto are unchanged, including replacement,
nonce retries and exact restores; those boundaries never perform a read.

The unsigned preflight entry preserves Lighter's declared authenticated read
role. Later nonce/approval and post-cancellation proof reads keep their existing
authentication, owner, handoff and deadline checks.

VENUE_STATE v2 appends canonical validated standalone native protective-order rows to the
existing account-index/tier header. No MARK value or receipt is imported. Each
row is revalidated against the receiver's ingested Store order through the same
native decoder before it grants exact modification or put-back authority.
Locally canceled identities remain suppressed, and a local modify (including an
Unknown) retires its old native parameters: replaying that metadata cannot grant
a second edit until the source supplies changed native parameters.
The header must name the receiver's already resolved account before tier or
native provenance is accepted, including address-configured receivers. A foreign
header cannot rebind the receiver and permanently retires pending edit bindings.
Each edit captures its initiating account at preflight and rechecks it after the
public read, before continuation and at final Claim; a return frame cannot revive
that edit. The existing pure reference/position/order veto remains unchanged. The source sorts rows for stable encoding. Missing native orders wait for their
ingested rows; a new source state replaces prior provenance. New readers also
accept the exact 11-byte v1 form, which exports when no originals are present
and grants no native provenance. Ordinary v1 writes remain supported; a v1-only
source cannot authorize a protective edit. Matching source/receiver builds are
required for v2; old readers reject the new version rather than guessing native
parameters. Frame publication, sparse updates and replays never renew a MARK
receipt. Lighter and lighter-robinhood use their own existing deployment host.

Native endpoint contract: the official Lighter Python SDK documents
[`accountOrders`](https://github.com/elliottech/lighter-python/blob/38a34672e7cb43c67d807ece25f125fda2dfdecf/docs/OrderApi.md#account_orders)
as lookup by client indices, covering recent active/inactive orders. The
[order model](https://github.com/elliottech/lighter-python/blob/38a34672e7cb43c67d807ece25f125fda2dfdecf/docs/Order.md)
names the native size, type, expiry, trigger status and group-link fields.
[Trigger conditions](https://docs.lighter.xyz/trading/order-types-and-matching)
use MARK. No live or demo orders are required by these controls.

## Native ModifyOrder

A complete native row waiting on MARK (`trigger_status=mark-price`), with zero
fills and explicit standalone links/flags, permits tx17 for native types 2/3/4/5.
The transaction changes amount/trigger and recomputes market-stop execution
prices from the new trigger's slippage band. Stop-limits retain their original
user limit. It cannot change side, type, reduce-only, TIF or expiry. Price-only edits
retain the original venue amount. A stale/missing mark or any source/position
change vetoes the final managed Claim. Frame-fed edits still refuse stale.

Market-stop edits recover their effective slippage percentage from the accepted
original native execution bound and trigger. Both guarded replacements and tx17
apply that percentage to the new trigger using the existing placement rounding.
This preserves the venue-rounded band; the original unrounded caller percentage
is not retained. Wrong-side or greater-than-50% bounds refuse before any write
with an unsupported-original-bound message; the user must re-place it. Ordinary
placement is unchanged; stop-limits continue to preserve their explicit limit.

Stop-limit edits refuse before any write if the kept sell limit exceeds the new
trigger or the kept buy limit is below it; equality is allowed. The user must
re-place with their intended limit. The PR 1 guarded path already refused all
stop-limit edits before cancel; legacy trigger-status controls pin that refusal.

The native [OpenAPI order enum](https://github.com/elliottech/lighter-python/blob/38a34672e7cb43c67d807ece25f125fda2dfdecf/openapi.json)
separates `mark-price` from `ready`, `na`, `parent-order` and `twap`; only the
first admits native modification. Legacy aliases keep the guarded path.

The transaction signs chain, type 17, nonce, transaction expiry, account, API key,
market, target index, base amount, execution price and trigger price. Native and
WASM use the same encoder; the device re-derives that exact field order from a
canonical typed statement, refusing unknown/unsigned fields and other scopes.
No new L2 attributes or order-version feature is introduced.

An accepted transaction returns the same target ID and refreshes orders; it does
not invent native provenance from the ACK. A refusal, 404, invalid nonce, rate
limit, timeout, transport loss or malformed ACK never cancels, puts back or
resends a tx17. Rate/405/429/408/5xx and ambiguous outcomes remain unknown and ask
the user to check Open Orders. A subsequent fresh source read is needed before
another native edit can rely on modified parameters.

Offline vectors in `tests/fixtures/modify-signing.json` were captured using the
[official Python SDK signer](https://github.com/elliottech/lighter-python/tree/38a34672e7cb43c67d807ece25f125fda2dfdecf/lighter/signers)
(binary SHA-256 recorded in the fixture) across chains 300/304/466324. The
[official tx17 field order](https://github.com/elliottech/lighter-go/blob/aefa96848a105d82a6bbb55d3d05663293a71713/types/txtypes/modify_order.go)
is checked against its hashes and signatures, including high market/target
indices and amount/price boundaries. The capture script takes only that pinned
library, uses the public fixture scalar and explicit nonces, and points its
client at loopback port 1; it never sends an order. Signing changes require a
dedicated security review before merge.
