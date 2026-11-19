# OKX legacy execution parity

## Missing public subscriptions

Native public WS `60018` refusals retire only an exact currently desired topic,
bound by returned `arg` or the native `<channel> subscribe for <instId> doesn't
exist` message. The adapter refreshes instruments once for each newly retired
topic and excludes that topic from reconnect replay for this engine's lifetime.
Cached market identities remain available to financial state; a refreshed
catalogue can omit the refused instrument without deleting those cached rows.
Private/business errors, unbound refusals and other public topics are unchanged.

## Historical fill fees

OKX v5 `fee` / `fillFee` is negative for a charge and positive for a maker
rebate. Historical fills negate that signed value into cost-positive
`commission`, matching the position journal's fee convention. Recent and archive
reads retain the same fee-field precedence, currency and missing-value handling.
`tests/history.rs` covers charges, rebates, zero and missing fees through both
endpoints. See the [OKX v5 fee definitions](https://www.okx.com/docs-v5/en/).

## Execution parity

The retained TypeScript adapter is the authority for trading rules. This change
adds outcome reporting without introducing whole-plan minimum, native cap or
wire-grid validation. Existing formatters and native execution owners decide
which writes are sent, retried, continued or compensated.

| Path | Legacy TS behavior | Rust scope |
| --- | --- | --- |
| Positive split tail below native minSz | `okx.exchange.ts:2266-2284` emits every positive remainder without a minimum-lot check. | Preserve the tail; no new minimum gate. |
| Row after the 20-row batch boundary | `okx.exchange.ts:2290-2315` sends batches serially. A later native refusal can follow an accepted batch. | Preserve native batch dispatch and report known outcomes. |
| Separate TP/SL children | `okx.exchange.ts:1758-1763,2181-2200,2318-2343` sends entry batches before full-input children, without native min/cap checks. | Match entry batching, child amounts and stable algo priority; remove the added child gate. |
| Ordinary amount/max/precision | `okx.exchange.ts:1751-1755,2226-2242` formats all ordinary rows and refuses nonfinite/nonpositive values before writes. | Retain matching preflight checks. |
| Opening conditional trigger/amount | `okx.exchange.ts:1759-1763,2162-2172` formats all algos and refuses a missing trigger or invalid amount before writes. | Retain matching preflight checks. |

Source paths above are under
`packages/safe-cex/src/__tests__/reference/okx/`; the same methods were checked
in the pre-move adapter at `4a3af89e6^`. The shared child derivation is
`packages/safe-cex/src/exchanges/base.ts:1446-1478`.

Known legacy issue: positive subminimum tails, later native rows and protective
children can partially execute before native rejection. This PR records that
behavior; it does not impose a new order-planning policy.

`safe_cex_engine::write_plan` supplies optional indexed placement error facts.
Its passive journal adds no native validity rules, and reporting failures cannot
block execution. A proven signature rejection retains its existing authorized resend
while the same logical row awaits its final outcome. Ambiguous released writes
stay unknown under the existing engine lifecycle. Cancel and compensating flows
keep their existing scheduling and native evidence; the optional placement
journal is omitted for those flows. Removed pre-existing put-back and resend
code is restored. The legacy TS snapshot proves protective cancel/re-place, not a
separate original-stop put-back algorithm.

After failure, normal and algo placement series stop, as TS's serial placement
helpers throw. Cancellation row errors inside a code-0 envelope continue; every
nonzero envelope stops, matching `okx.api.ts:77-82`. Regular and algo cancellation
tests cover both paths; WritePlan does not control dispatch.

Sequence pins require all entry batches before SL/trailing/TP algos. Children
retain the original input amount, so a child refusal leaves the accepted entry
prefix and stops later algos. Per-lot entry/SL/TP interleaving is not TS parity.

The mixed-algo pin asserts trailing priority only. TS `okx.exchange.ts:2203-2208`
sends `closeFraction=1`, omits `sz`, and uses the input price as `callbackSpread`.
Inherited Rust behavior sends native `sz` and a distance from the last price;
this remains a documented deviation, covered separately as a native control.

This PR remains stacked on #3348; re-merge master after that dependency lands.

Stop-after-Unknown is a recorded parity exception (Sean/daxx, 2026-11-08).
When a native response leaves a write unproven, the rest of that action stays
unsent, including signature-looking replies and protective replacements. The
terminal preserves accepted IDs and indexed placement facts, identifies the
unconfirmed request, and says `The outcome is unknown. Check Open Orders`.
The venue emits one `order.outcome_unknown` event with the command, accepted
count and unconfirmed request. Late acknowledgements cannot restart the action
or emit another terminal/event. Proven signature refusals and definite native
cancel-row refusals retain their existing resend/continuation behavior.

The nine path pins live in `tests/support/unknown_exceptions.rs` and
`tests/protective_amend.rs`, covering
split tails, later ordinary batches, TP/SL children, standalone algos, valid
ordinary and conditional runtime writes, normal/algo cancellations,
signature-looking ambiguous replies, and protective cancel/re-place. Their
comments cite the legacy TS continuation each exception overrides. Placement
pins check placed, Unknown and unsent journal elements; cancellations retain
the deliberate optional-placement-journal omission and the engine's call facts.

KNOWN-ISSUE (deferred to #3359): engine-sealed Unknown telemetry. HTTP
408/429/5xx pins still assert the stop, Unknown outcome and the engine's canonical
text; only their telemetry assertion is deferred. No shared engine code is
changed to add this venue companion event.

## Entire-position TP/SL (AGT-68)

OKX keeps one entire-position (`closeFraction=1`) TP/SL algo per position and
refuses a second with `51088`. A full-position SL or TP whose position already
holds a single-leg full algo of the other kind joins it: one
`/api/v5/trade/amend-algos` carrying only the new leg's trigger price and
trigger type, with `cxlOnFail:false`. No size or execution price is sent
(51523/51524), so a refusal leaves the existing leg in place and never cancels
or re-places it. The command completes with that algo's native ID. A full SL and
TP in one command become one `oco` algo. Partial-size legs, a position already
carrying both legs, and other positions are placed as before. Legacy TS sends a
second `closeFraction` algo and gets `51088`; this is a recorded deviation.
Live acceptance of adding a leg through amend-algos is not yet verified; OKX
documents leg deletion there, and a refusal is a definitive `Rejected`.

## Native write refusal diagnostics

One ACK decoder borrows plain messages and owns escaped messages. Owned text
is accepted only for the existing failed-empty-envelope recovery; success and
nonempty-message envelopes retain their original refusal and empty defaults.
When a failed settings envelope has an empty `msg`, a sole failed ACK supplies
the native code and message. Mixed settings rows retain Unknown and include the
failed-row reasons; they never prove the whole call was refused. Nonempty
settings-envelope messages and successful ACK handling are unchanged.

Cancellation keeps `51400` as a satisfied intent, quiet in code-0 continuations
and behind any genuine sibling failure. If a nonzero envelope has no message
or other row failure, its already-absent row supplies the native code/reason
instead of an empty `OKX 1:`. Complete identity-validated cancellation batches
also retain passive per-order outcomes on failure. Dispatch, retry, store and
aggregate outcome rules are unchanged. Missing or unusable row diagnostics are
explicitly unavailable; contradictory acknowledgements remain Unknown.

`tests/write_errors.rs` records the [OKX v5 response shape](https://www.okx.com/docs-v5/en/#order-book-trading-trade-post-place-order)
and pins decoded diagnostics, accepted siblings, unchanged settings on failure,
and no extra writes after a failure. The fixture uses the public documentation's
example code/reason, not a private account response.


## EEA private startup

Private EEA startup authenticates the existing signed account-config read before
signed account-instrument discovery, including when public markets are warm.
Its parsed settings retain the original read authority and apply only after the
catalogue settles; config alone proves no balance, orders or positions readiness.
Both EEA aliases retain the EEA origin and demo headers where applicable.

Config and keyed catalogue reads share the native authentication-code policy:
one freshly signed retry after `50113`, then a permanent typed refusal on a
second consecutive answer for that read and credential. Transport, rate and
other replies reset the streak; retry floors and admission deadlines do not grow.
Pending EEA config reads retain the HTTP rate/transport diagnosis on unproven
responses, even when their body contains an authentication code. Such a reply
cannot become a permanent credential refusal in the shared hosted probe.
Complete JSON catalogue replies without a data list use the same signature
assessment as list-bearing replies: non-authentication errors reset the streak;
genuine signature answers still receive only one retry before permanent refusal.
Public/frame-fed, non-EEA and already-configured refresh paths retain their
existing behavior. No startup step places or replays a trading write.

## Ordinary-order history coverage

An explicit range older than seven days reads ordinary recent history first,
then the three-month archive under the same read root, deadline and 100 ms
continuation pacing. Merge identity is native `ordId`; recent rows win duplicate
archive deliveries. Recent-only requests retain their existing request count.

OKX retains unfilled canceled orders for only two hours, and the archive omits
that population. Queries whose lower bound reaches the two-hour boundary by completion, or has
no lower bound, emit the existing owned `retentionLimit` marker, including empty
results. The check includes time spent fetching pages. A later
page failure retains its more specific incomplete reason and accepted prefix.
See the [recent](https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-order-history-last-7-days)
and [archive](https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-order-history-last-3-months)
source contracts. `tests/history_order_retention.rs` exercises their combined read.

The scope remains ordinary orders. Algo/protective history is a separate source;
position-scoped rows with no native size need an explicit nullable-size model
before they can be included. No size is fabricated and no archive is represented
as a complete census of all order populations.

## Unresolved historical order markets

An order can outlive its instrument's current catalogue entry. An in-window row
that cannot be normalized marks the owning history command `coverageGap`, even
when no orders can be mapped; it cannot prove completed coverage. Available
mapped orders retain their sizing and identity. An unrelated instrument or a row
outside the requested time window does not mark that window incomplete.
Symbol-scoped order history with no market metadata refuses locally rather than
returning a successful empty result. No symbol or contract size is guessed.

## Order-history financial projection

Ordinary derivative order history retains the native `pnl`, `fee` and `feeCcy`
from [OKX orders-history](https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-order-history-last-7-days).
`realizedPnl` is the venue's gross order close PnL, excluding its fee. It is not
the position-history net `realizedPnl`; no fee or funding is added to it.

For SWAP and supported EEA FUTURES, native `fee` is an accumulated signed
fee/rebate total: a negative charge becomes positive-cost `fees`, and a positive
credit becomes separate positive `rebate`. `feeAsset` preserves `feeCcy`; a
credit also carries that currency in `rebateAsset`. The reported aggregate
cannot reconstruct separate gross fees and rebates across the order's fills,
so the projection does not invent the unreported counterpart. No monetary
amount is converted with contract size or price, including BTC-settled orders.

Missing, null, blank, malformed and non-finite monetary strings remain absent;
a reported zero remains zero. Non-financial order fields, query scope, paging
and write behavior are unchanged. Adding these fields does not promote the
orders axis from unmeasured: an independent financial check is still required.
