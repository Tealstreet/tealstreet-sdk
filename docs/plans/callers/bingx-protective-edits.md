# BingX protective edits

An edited standalone STOP_MARKET or TAKE_PROFIT_MARKET retains its original
reduceOnly flag in one-way mode, or closePosition for a full-position stop.
This fixes the live loss of closing authority: a replacement must never become
an opening order merely because the edit reformatted it.

A complete native snapshot supplies the original quantity, side, position side,
trigger and workingType, closing flags, stopGuaranteed, priceProtect and exact
positionId. Every protective edit (a standalone STOP_MARKET/TAKE_PROFIT_MARKET
trigger move, a TRIGGER_MARKET move or a reduce-only LIMIT price move) is a
guarded serial cancel, confirm, place: DELETE the original, require the
cancellation proof below, then POST the replacement. LIMIT quantity edits retain
native amend; protected parent LIMIT price edits remain refused; unsupported stop
size edits remain NotSent. Only an unguarded LIMIT price move still uses native
cancelReplace.

Guarded edits used native cancelReplace until #3828. Its live reply nests the
cancelled order under `cancelResponse`, which the proof never read, so every such
edit surfaced an Unknown cancellation after BingX had replaced the order; its
separate cancelResult/newOrderResult also never established rollback. Serial
cancel/confirm/place replaced it rather than a further decoder.

All destructive managed Claims, including signature/timestamp re-signs, evaluate
one pure venue veto. The rounded trigger uses its native Mark or Last source;
its genuine local per-field receipt must be no older than 5000 ms, equality at
the trigger is crossed, and missing or sparse-retained references refuse the
edit. The original Store order must remain unchanged and unfilled; a closing
stop additionally requires a position on its protected side. A veto unwinds the
Claim and reports the cause as NotSent, leaving the original open.
Receipts come from actual Daily Last, Premium Mark and Live trade/fair-price
messages, including unchanged values. Delayed seed publication cannot re-date
an earlier field, and a Live row without tradePrice cannot renew Last. REST row
receipts become usable only after the whole response envelope is accepted.
Receipt times are monotonic per symbol/source; a ticker mismatch or frame reset
cannot let an older pending reply replace newer evidence. Frame resets invalidate
proof without discarding the clocks that fence earlier reads.
The <=5 s genuine proof intentionally remains stricter than legacy TS
replaceTriggerOrderPrice, which optionally checked Store Mark without an age gate.

A target-matching terminal cancellation with explicit zero execution is required
before placing; an empty code-0 object, missing execution proof, a foreign
identity/symbol or a contradictory streamed fill is Unknown and places nothing.
Complete native parameters are preserved when available, including closing and
guaranteed-stop flags and the original quantity. Native flags that cannot be
preserved invalidate amendment metadata before cancellation.

Nothing is ever placed automatically after a failure. A refused cancel leaves the
original open. After a proven cancellation, a definitive replacement refusal
reports typed Unprotected with a visible re-place warning; an unclear replacement
is Unknown with Open Orders wording. Neither resends nor puts the original back.

HTTP 429 and native 100410 on guarded legs are Unknown, even with a refusal-shaped
body. An Unknown leg never resends and never claims protection. Unrelated gateway
classification is unchanged. Before each guarded leg, present_plan_failure
registers its current unknown words, so an engine-sealed 408/5xx/transport loss or
timeout cannot present a stale replacement refusal. Outcome and protection remain
engine-owned.

VENUE_STATE exports version 2 with the typed native original and existing
observation. Readers also accept version 1 from older sources, loading mode and
observations without the typed native original. Unknown versions fail closed. The real native
snapshot -> frame control pins this contract. Frame arrival, replayed snapshots,
time-only updates and sparse rows never grant local reference freshness.

Before admitting a protective edit whose requested Last/Mark receipt is absent,
stale or inconsistent with the Store, the native command obtains one unsigned
public Daily/Premium read under its existing preflight owner and generation.
It has a 20 s bound and one attempt, with no credential, signing, retry or
cross-source fallback. Omitting the symbol parameter preserves the existing
streamed all-contracts array shape; only the exact native target row is retained
and the existing strict Daily/Premium decoder is reused. The source is never
inferred from a different ticker field. The actual row receipt is not re-dated
when the envelope finishes; an invalid, duplicate, missing or >5 s old target
cannot authorize any destructive leg. A newer genuine live receipt is retained.

The command captures its original immutable edit before the read. Once the full
envelope succeeds, it validates the captured guard and hands that same preflight
to the existing write plan. Changed/filled orders, a vanished protected position,
a crossed trigger, a retired owner or a failed read leave the original open with
NotSent. Every later managed Claim still performs the unchanged pure guard,
including re-signs; expiry at that boundary never schedules
another read. LOCAL/COSIGNED frame-fed page engines can therefore obtain genuine
proof without trusting frame arrival or changing the VENUE_STATE format.

Fixture changes preserve the positive wire assertions while supplying explicit
unfilled target cancellation proof in raw-trigger, last-price and moved-stop
controls. The old attached-bracket fixture with no protected position now expects
NotSent; its positive open-position half still checks the replacement. A definitive
bracket replacement refusal now asserts typed Unprotected instead of an untyped
lost-stop string. Lost or unclear replacement controls assert Unknown and Open
Orders wording. The original reduce-only regression retains its wire assertions
and now supplies executedQty=0 in the cancel acknowledgement.
