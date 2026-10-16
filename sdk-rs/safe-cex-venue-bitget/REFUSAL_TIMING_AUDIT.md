# Typed-output refusal timing

Scope: implemented Bitget mapper/fold paths after e8b60fd97f, following the
canonical closed-typed-output-boundary timing ruling at 120fd0c453. A source
TypeError and a Rust RepresentationError are different outcomes even though
both fail. The witnesses compare name and message, not just is_err().

| Path | Refusal point and evidence |
| --- | --- |
| map_order | Private PendingOrder carries all table hits through source field computations and every attached leg. Only then does project decode shared typed fields. 36 source cases combine each of twelve inherited names in status/side/force with a valid stop-loss leg and a later throwing take-profit value. The later source TypeError wins. |
| map_inverse_order | Same deferred projection structure, independently executed against the inverse class body. Another 36 cases pin the later attached-leg error after the clock and market lookup. |
| map_bitget_bill → map_bitget_bills → fetch_bill_page | The chosen optional-category refusal retains the record with category None; it is not an early Err. Twelve whole-page source cases put each inherited category before a null bill. The null TypeError wins and the page does not return a partial ledger. |
| map_bitget_fills | Source predicate guards construct only supported typed legs/sides. Errors propagate from source coercions and supplied operations; no typed-output RepresentationError short-circuits this batch. |
| position mappers and merge_rest_positions_with_fresh_ws_state | Normalizers select typed enum values by source branches; the fold selects already-typed Position values. There is no per-iteration typed-output refusal. Supplied operations remain their own boundaries. |
| ticker handler | Shared inherited reads reach the intended native patch operation; there is no typed-output refusal to mask a later row. Separate notification and poisoned-chain FINDINGs remain open and are not discharged by this audit. |

The order mappers above operate on ONE source order, potentially producing
several attached legs. Their still-pending outer exchange batch callers are NOT
covered. When those callers are implemented, an internal representation refusal
must be retained until later rows have had their source-visible effects and
errors evaluated; a plain fail-fast collect over per-row mapper results would
not establish that contract.

Production ordering was already correct at the audited entry points. New
witnesses add coverage without changing a row count or claiming any outstanding
representation loss repaired. Ordinary source exceptions retain immediate
source timing; only a Rust typed-output refusal needs deferral.
