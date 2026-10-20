# Trading and stream race audit fixes

Branch: `feat/rust-order-races`; base: current `origin/feat/rust-rewrite` (audit references `df2d6a4a03`).

Scope is the requested trading medium/low items in `RUST_AUDITS.md`. Binance and Bybit are owned by the separate `feat/rust-audit-binance-bybit` lane. No TS venue deletion, cutover/default change, budget increase, or live private writes.

## Phases

1. Trading: reverify M1/M2/M3/M4/M11 and low Backpack positions/OKX status/KuCoin IDs. Add shared engine order-mutation admission only where it safely preserves command completion, and regression tests for pending placement, partial fills, overlapping amend/cancel, ambiguous writes and read errors. Review and commit coherent fixes.
2. Private reconciliation: independent stream execution baselines for Bitrue/BingX and Aster/WOO fallback, BloFin session cutoff, Lighter late executions. Regression tests must interleave REST and WS rather than assert final state alone. Review and commit.
3. Public recovery: engine explicit book invalidation; WOO prevTs, Kraken Spot checksum, Aster/Lighter recovery. Preserve legitimate empty/one-sided snapshots. Test resync and recovery and review.
4. Gates: native quality/tests, isolated and wasm quality, root quality, combined/selected WASM and Binance plus every touched venue public/private proof. Fixed budget remains 1,258,291 bytes. Record exact source, results and any limitations.
5. Landing: merge latest landing branch, repeat affected gates, push/open PR into `feat/rust-rewrite`, post correctness sub-agent reviews, fix/repeat, merge only green CI, cut clean follow-up branch. Strike only findings actually fixed in this PR, preserve unrelated findings.

## Progress

- Read origin audit and port process before editing; fresh branch created cleanly.
- Initial re-verification delegated by disjoint account/public/trading scopes. Shared order admission and cancel-all gaps remain with primary agent.
- Every phase requires a sub-agent correctness review; workers must not revert other scopes.

### Private reconciliation checkpoint

Reverified and repaired REST/stream execution baseline races for Bitrue, BingX, Aster and WOO fallback; BloFin pre-session executions now seed baselines without emission; Lighter processes late delivered executions once. Seven deterministic interleaving regressions added. Focused six-venue account suites passed (136 before the extra WOO gap regression). Independent review found and repaired WOO blind-gap pricing/quantity inference; reciprocal public review also passed. Full crate quality and final-source gates remain outstanding. No TS venue was deleted.

### Trading/public checkpoint before landing refresh

Implemented and peer-reviewed: shared feature-gated mutation admission across concurrent/preplanned cancel+replace adapters; serial-intent adapters retain FIFO behavior; seven affected adapters also track pending placements and ambiguous outcomes. Retained native acknowledgement routing closes pre-stream cancellation, including regular/conditional/TPSL/UTA families. SDK cancelAll now retains attempted placement scopes so it reaches native cancellation with an empty store. Explicit partial-business rejections remain cancelable; missing acknowledgement remains uncertain. Complete fresh REST snapshots use causal read epochs and venue snapshot-generation fences; Phemex has no causal REST orders census, so ambiguous sent placement conservatively prevents symbol/global cancellation until recreation (known-native individual cancels remain available).

Backpack remaining-size/cancel-racing execution and 404 preservation, KuCoin collision-resistant fallible IDs, OKX partial-effective/nonfinal status, OKX/Orderly ambiguous-write refreshes implemented and independently reviewed. Private followup seeds BloFin startup cumulative/quote baselines once without letting periodic REST consume stream executions (17 account tests and strict Clippy pass). Wrapper regressions 105 tests plus typecheck/lint pass.

Public fixes peer-reviewed: empty invalidation frames, dropped-clear snapshot recovery, WOO prevTs, Kraken Spot exact-decimal CRC32, Aster/Lighter recovery clearing and stale-REST fences. Engine focused tests 90+ pass, 74 public cases pass. Intentional TS corrections include WOO continuity, Kraken Spot CRC, Backpack 404 preservation, documented in gate comments; replay comparisons are not weakened.

Exploratory selected Paradex build measured 1,258,877 bytes, exceeding fixed 1,258,291 by 586. This is a real gate failure, not accepted evidence. Replace global snapshot subscription bookkeeping with the existing venue book iterator contract, then rerun final-source budgets.

Next: checkpoint all verified work, merge current origin landing (includes other lane PR1338), reconcile shared feature/command changes, lean snapshot repair, strike verified audit items, freeze source and run final gates. Existing focused logs are developmental evidence, not final frozen-source results.

### Lean recovery and merged-source review

Merged current parity landing in `c2eb6aa8ca`, preserving prediction/polling feature isolation, remote-sign request events and KuCoin Spot. Removed global snapshot subscription allocation in favor of the documented `Venue::books` subscription census: snapshots emit clears when `book()` is unavailable. Non-Binance/Bybit iterators now comply; their lane was notified through Aimux of its two readiness-filtered iterators. Engine snapshots retain valid one-sided books and clear stale consumers even after a missed invalidation frame. Developmental engine tests (95+), WOO/Kraken/Aster suites (354) and strict Clippy pass. Merged exploratory Paradex size returned below the unchanged budget; final frozen-source budgets remain required.

Integration review fixed causal-token leaks on rejected/expired delegated signing, duplicate unsupported-command IDs, and late signer errors erasing active HTTP provenance. Five focused signing regressions and strict Clippy pass. Production-impact guidance from Sam governs all scope decisions; lifecycle differences alone are not a reason to change code.

### Orderly priority addendum and final timeout review

All six Orderly brokers accept standalone SL/TP with the application's matching `price`/`triggerPrice`; different prices and opening conditionals still reject before writes. Proven independent trailing/quantity protections do not block a new full-position leg. Mixed-parent or contradictory provenance rejects before destructive cancellation, preserving sibling protections. SDK cancelAll uses native unscoped cancellation even with an empty store. Native addendum, wrapper regressions, typecheck/lint and independent reviews passed.

Scoped cancelAll attempts every retained market before reporting aggregate failures, so a rejected earlier symbol cannot suppress cancellation of later acknowledged orders. Ambiguous write reconciliation additionally covers HTTP408/5xx, OKX top/per-row50004, and malformed success acknowledgements. Definite business errors retain rollback semantics; independent Orderly cancel families continue without replaying ambiguous writes. OKX125 + Orderly93 native tests and strict Clippy passed. Config-specific lost mode/leverage confirmation is not claimed by the optimistic-order repair. Two concurrent history fixtures now use distinct command IDs, preserving all timing/assertion oracles.

Prepared and independently reviewed final manifest: 27 selected builds plus combined integration build; 105 replay proofs (40 public,65 private); fixed1,258,291 byte budget. Fourteen SDK WASM suite files include12 actual compiled-engine suites and2 independent fixture suites. These are planned counts, not completed results. Native/root quality, serial gates, PR correctness review and hosted CI remain to be run on the final committed source.

### Consumer landing integration and final fixture audit

Merged consumer landing PR1342 in `a6277c91a3`, preserving native shared-feed suppression/restoration and selecting the shared book for snapshots and invalidations. Independent merge review passed. A new regression checks populated shared-book snapshots, missed clears and one-sided recovery when the venue exposes no native book. Final gate coverage includes the newly landed consumer suites; source and runner hashes are frozen only after review.

Workspace preflight identified four outdated test scenarios, not accepted failures: Apex concurrent command IDs now differ, GRVT distinguishes empty subscribed placeholders from ready books, Hyperliquid asserts the earlier shared rejection, and WEEX proves overlap rejection/lock release/retry ownership plus immunity to a late duplicate failure. Apex trading24 and the GRVT/Hyperliquid/WEEX public/trading106 tests passed developmentally. The full final workspace gate remains required.

Reverified Orderly M4 without adding a lock: queued commands are planned on activation, replacement ACKs wait for authoritative child visibility and cancellations wait for terminal/fresh absence. A native child amend invalidates protective provenance, so ACK-before-stream cannot authorize a stale parent rebuild. Existing lost-pair/FIFO/bulk-preemption scenarios cover the production windows; lifecycle structure is retained. This is a source review conclusion, not a newly executed live-key proof.

Final runner review passed after consumer integration: planned217 serial steps,27 selected builds plus combined,105 replay proofs (40public/65private),16 SDK WASM files (14compiled-engine/2independent fixtures), and5 consumer WASM files across server/MCP/CLI/React Native plus MCP package build/smoke. Paradex is first selected artifact to expose the tightest budget early. Native16 and root3 quality commands precede serial execution; all remain pending until frozen-source receipts pass.

### Final-attempt review: definitive Phemex rejection

Frozen source `daa0bf79d0` passed all 16 native quality commands (2,851 workspace cases plus 482 overlapping isolated-feature invocations), and root typecheck44/lint28/test27 tasks passed; each overlapping SDK test target passed7,797 cases. Independent raw-receipt reviews passed. Serial execution built the combined artifact, then was explicitly stopped during SDK tests when review found a real Phemex regression. That serial receipt is FAIL, not accepted final replay/size evidence. Logs and the aborted receipt are retained under `/tmp/order-races-final-*`.

A proven rejected placement incorrectly poisoned permanent Phemex cancel-all uncertainty. Reproduced RED with an existing live order and a complete rejected create ACK, then repaired classification: only exact correlated terminal Rejected rows or eleven documented balance/parameter/risk codes with no order data resolve the current placement. Generic response routing preserves HTTP/error-envelope rejection and original error context. Malformed, identity-mismatched, suspended, unknown/duplicate/routing/non2xx outcomes remain fenced; earlier native ACK routes and older uncertainty survive. Seven scenario regressions include queued cancellation, partial batch Init ACK, and negative controls. Full Phemex105 tests and strict Clippy passed; independent review passed.

Unknown cancellation outcomes now report the same actionable uncertainty error for deferred and later cancel-all calls, rather than a generic in-flight error. The existing scenario proved RED before the message fix, then106 engine feature-on tests and strict Clippy passed; independent review confirmed admission/state unchanged. This checkpoint supersedes the earlier frozen source. All final quality/replay/size proofs must use a new frozen file/prefix; old receipts are preserved and not relabeled.

### Backpack fixture contract correction

Source `01eefe752c` passed all 19 native/root quality commands and independent receipt review. Serial checks passed combined SDK443 tests across16 files, consumer20 tests across5 files, controls,48 replay proofs and10 selected artifact budgets. The run stopped FAIL at Backpack private: the old generator returned `{}` for successful cancel acknowledgements, whereas the published HTTP200 contract returns the full cancelled order (HTTP202 is only accepted). The stricter amend parser correctly refused to replace an order on that incomplete fixture. The failed log/tape and original receipts remain untouched.

Corrected only the two DELETE response bodies to complete native400/401 Cancelled rows with original submitted fields and cumulative execution0; requests, event order, parser, replay and scoring are unchanged. Two generated tapes are byte-identical, comparison against archived input confirms exactly two changed response records, Backpack18 native trading tests pass, and the unchanged private gate passes against the corrected input and exact saved artifact. Independent fixture and tail-ACK reviews passed.

A separately reviewed continuation may carry only the original111 successful prefix steps (48 proofs) through Backpack public, preserving their original HEAD/source/log/artifact provenance. It must prove the complete runtime/input hash inventory differs only in this Backpack generator plus this named document, rerun Backpack tape generation and the original106-step suffix (57 proofs), and require fresh19 quality commands at the new frozen HEAD. Any other runtime/TS/replay/scoring/build/config/fixture change invalidates that carry-forward. No prior evidence is relabeled as newly executed.

### Startup/broker landing integration

The reviewed two-epoch continuation at `d14b8529d2` passed Backpack private and BloFin public/private, then was deliberately stopped during the next build after discovery of landing PR1347 and its conflicting startup/broker changes. The V3 receipt remains FAIL/incomplete; prior V2 failure and all raw evidence remain unchanged. No partial continuation is final gate evidence.

Merge current landing `702a62050c` before submission. Preserve both retained ACK-before-stream cancellation and landing explicit unknown-ID errors/broker attribution, plus both Orderly and KuCoin unscoped SDK cancellation. Conflicts are reviewed per owned venue; combined source requires a new full frozen quality/replay/size run. Earlier carry-forward admission is invalidated by these runtime/harness changes.

Combined merge review passed independently: SDK union and ACK/native cancellation preserve both parents; Bitget auto-merge preflight was corrected to retain ACK-only routes. Added mixed cached-ACK/unknown-ID regressions for Bitget/Bitrue/BloFin prove no partial writes and usable retained routes. Focused merged trading91, Bitrue/BloFin152 and SDK54 tests pass. A shared BloFin test helper needs a narrow unused allowance in non-trading integration crates; runtime guards are unchanged. KuCoin Rust authentication documentation now distinguishes configured Futures public signing from unsigned Spot/public-data-only requests.

### Final wire/leverage/Binance-Bybit landing integration

At `7e64a408a1`, the full V4 run passed all19 native/root checks,217 serial steps and105 replay proofs (40public/65private). All27 selected Node/Web artifacts fit the fixed1,258,291-byte budget; maximum Paradex1,222,343. SDK444 and consumer20 WASM tests passed. Independent raw-evidence audit passed; prior failed/stopped attempts remain preserved. PR1353 opened into `feat/rust-rewrite` with these source-specific numbers.

Landing advanced during the run with wire/leverage/Binance-Bybit PRs1345/1348/1349 (`c44f22bd8e`). Merge their revised frame readers, side-aware leverage commands, mode state and cancellation fixes before PR completion. Preserve both parents' audit closures/tests plus this lane's ACK races, protective-order and timeout behavior. The old full PASS remains valid evidence of its original source; a new frozen run is required on combined runtime changes. Adversarial PR reviews are pinned until integration is complete, then inspect the resulting head.
