# OKX protective amendments preserve protection

SDK and NX/server callers keep the existing `AmendOrder` command. Standalone
TP/SL now uses `/api/v5/trade/amend-algos` with `cxlOnFail:false`, retaining the
native trigger source, market/limit execution price, zero-size full-position
semantics and untouched linked sibling fields. Native `trigger` rows use the
trigger fields; `conditional`/OCO rows use their TP/SL fields. A refused native
amendment keeps the old protection and emits no cancel or replacement.

Trailing stops retain cancel/place because OKX excludes them from amend-algos.
The native account reducer retains original size, activation, callback ratio or
spread, position side, margin mode, client ID, currency, tag and reduce-only.
A final `http_release_veto` checks the live source, market size limits and a last
price observed within five seconds before cancellation. An already firing native
trailing trigger refuses the cancel; activation prices are not fixed SL triggers.

Frame-fed writers acquire native parameters with a five-second private GET before
the mutation, under the same command/serial owner and existing order lock. A
trailing edit with stale market data also obtains a five-second ticker GET before
canceling. That actual quote is retained only by this amendment for the final
five-second veto, without republishing market data. Algo-detail `last` is the
price while placing, not a current quote, and is never freshness evidence.

A failed, foreign, terminal, expired or concurrently outdated source never cancels.
No lookup, freshness veto or optional wait is introduced after an accepted cancel.
A late cancel ACK still releases its replacement.

After a definitive venue refusal of the replacement, the original trailing parameters are
submitted once as a put-back. Successful put-back reports typed `Restored` on the
failed amendment; a refused put-back reports typed `Unprotected`. Unknown
replacement or put-back outcomes never produce another stop or claim protection;
callers retain the existing Unknown/reconciliation handling. Existing order locks
exclude a concurrent edit, cancel or cancel-all throughout cancellation and put-back.

The refusal tests that expected standalone TP/SL cancel/place now assert native
amend refusal with the old row retained; destructive fallback controls cover
trailing cancellation, exact restoration and failed/unknown restoration instead.
No caller wire change, dependency, capability, cap or deletion prerequisite is added.
