# Binance class owner review

PR: https://github.com/Tealstreet/tealstreet-next/pull/7851

The slice starts at integration `ac0f5fccf2`. The inherited unfinished owner
assemblies were checkpointed, then cherry-picked onto that fresh integration
parent. No unrelated feature branch was merged into the slice. Three reviewers
used separate detached checkouts for the subsequent review rounds.

| Review | Finding and disposition |
| --- | --- |
| Public | Fixed omitted ticker symbol aliases, then fixed collision ownership using the authoritative Store symbol map. Native collision: `btc.last=12.3`, displaced `eth.last=0`. Final public review: MERGE at `9e520a1077`, including the ID index preserving the same last-match policy. No benchmark claim. |
| Public boundary | Actual TypeScript callback observes lone unit 55296 once; native parser drops the whole frame. Nine dispatch/socket class notes cite [shared string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary) and [surrogate observability](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). This remains divergent. |
| Private | Fixed swallowed synchronous Futures/PM open errors, retained callback Base leases, and lost discarded async rejections. Follow-up found retained unpolled rejections also went unreported. The final observation checkpoint reports without requiring handle release, suppresses observed rejections, and reports once. Final private review: MERGE at `9e520a1077`. |
| Exchange | Fixed copied retry guards, incorrect reconnect snapshot route, uppercase header lookup, signer injection, public socket registration, async/timer root leases and the missing Futures ledger-source field. Replaced item-declaring macros across all new wrappers. Source Infinity is asserted before serialization. Final exchange review: MERGE at `b80fa59337` for the explicitly divergent slice. |

All reviews were local sub-agent reviews. Source probes were read-only. Reviewers
did not run Cargo, extractor or mutation gates; those results belong to the
parent's SHA-bound gate sheet. Initial review verdicts were DO NOT MERGE and
triggered the repairs above; no human approval gate was used.

Accepted omissions in the three exchange class rows remain:

- `ExchangeState::store_field` projects native Infinity through JSON to null;
  the native constructor test pins this before any source comparison.
- Balance reads remain snapshots while the source alias observes a later 4.2;
  see [the canonical alias declaration](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).
- Executable `headers.get` and inherited generic core `Exchange` dispatch are
  not bound by this owner interface.
- Explicit disposal retains the graph through `ExchangeState -> Base.sockets ->
  BaseWebSocket.parent -> ExchangeState`, and Futures also through
  `ExchangeState -> imports.guards -> observer -> ExchangeState`. The named
  native constructor test observes the retained Weak owner after disposal.
  Independently retained child/receiver handles require further lifetime work.

The MIXED row notes inventory these findings under
[the canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).
The twelve-row closure is a census result with concrete Rust items. It is not a
whole-venue parity proof or a claim that the accepted omissions are fixed.

The private reviewer subsequently approved the isolated dependency-loader and
mandatory source-control-test delta at `fc427e57ab`. A final read-only evidence
audit independently confirmed the twenty-file keyed census, all five restored
Rust control hashes, and unchanged compile inputs between `f14b814e11` and
`fc427e57ab`. That audit made no claim for the then-running full test suite.

The final addendum audit checked the completed raw logs: 71 result blocks,
274 passed, zero failed/ignored, all seventy independently enumerated integration
targets, and the named numeric mutation failure followed by successful
restoration. The restored source hash and empty source/test diff matched. No
material findings remained in that evidence scope; the reviewer ran no gates.
