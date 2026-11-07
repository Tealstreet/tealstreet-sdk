# Architecture and replay-evidence re-audit

Base: landing `b6cb612bc7c7c611a17dff5900e6977a7e8b5ab8` (#1688).
Integrated landing `c568c1745a9` preserves the later GRVT book repair and
caller documentation. Integrated #1694 at `e87107621f6` supplies the final
clock-policy regression and narrow WEEX declaration cleanup. Later Paradex
#1687 at `90df20d2f14` is also integrated; its new evidence plan is indexed.
Scope: the Architecture and Gate honesty sections of `RUST_AUDITS.md`.
Use current code and actual merged PRs; historical audit counts are not a
current inventory. Production impact determines whether a remaining finding
needs a code fix, a cutover prerequisite or an owner decision.

1. Re-verify every Architecture bullet against shared engine helpers, models,
   feature declarations, hosts, venue implementations and tests. Distinguish
   remaining venue-specific semantics from stale duplication counts. Inspect
   manifest/script/config comments and documentation inventory without deleting
   useful evidence or changing trading behavior to satisfy an old shape.
2. Re-verify Gate honesty against current comparison/fixer/measurement policy,
   CI coverage, budgets, tape-time receipts and complete-output proofs. Pin
   merged PR evidence for stale findings. Preserve FAIL versus exact declared
   known failure, repeatability versus parity, incomplete evidence, missing
   metadata, fixture provenance, independent-oracle limits and live prerequisites.
3. Coordinate with codex-mxb4mq, which owns the harness sweep. This first PR
   changes documentation only: this evidence ledger, the two audit sections,
   stale port-roster entries and a single plan/evidence index. The overseer
   decided to retain the documents and index topic/status/read trigger/file,
   rather than consolidate or delete them.
   Do not edit its harness, tapes, declarations, clocks, comparators or policy.
   Cite its actual merged sweep receipts. Record any remaining
   harness-owned gaps and owner decisions with exact boundaries.
4. After the evidence PR, fix any demonstrated SDK/engine-owned gaps in fresh
   branches, batched by behavior. Write a concrete short plan before each batch,
   reproduce the failure with focused regressions, preserve external contracts,
   run relevant checks and CI, obtain independent reviews, then separate guard
   and exact-head merge. Naming/feature refactors must not change computations,
   command routing, public schemas or WASM budgets. Never delete TS venues or
   executable TS performance references before cutover.
5. Track completed landing-push browser measurements introduced by #1688.
   Once at least three complete real-landing runs exist, examine all samples
   and runner/browser provenance and propose a cold-start threshold to the
   overseer. Do not implement a threshold. Own-head dispatch evidence and
   skipped/cancelled/incomplete runs do not substitute for landing baselines.

Verification: source/history and merged-PR pin checks; focused existing controls
for assertions cited in the docs; links and diff checks; independent plan and
evidence review. No local release WASM builds, replay sweeps, timing matrices,
live trades, deployments, NX edits, budget changes or release automation.
Require green PR checks and a fresh separate trial/revert guard under the lane
rules. Report every actual merge pin to the overseer.

## Re-verified inventory

At the pinned base: ten Architecture bullets; six implemented Gate honesty
bullets plus six evidence limitations. The old strike counts are not an open-work
count. The current harness requires 32 suites / 110 exact cases and has 50 aliases.
All 50 use the effective deterministic private-clock policy: 49 explicit flags,
plus polling BreakoutProp. This is a source-policy observation, not proof that
every private result is correct. #1694 now pins the exact 50-name catalog and
effective policy in a regression, without introducing a new clock implementation.

The integrated index covers 152 tracked paths under `docs/plans`, including
itself and this ledger. Its status column describes document use,
not blanket feature completion. Historical plans/JSON receipts remain intact.
The stale in-progress Bitunix, KuCoin/Phemex/Orderly, Lighter and family-build
roster rows are refreshed using actual first-parent merge receipts.

## Architecture evidence

| Finding | Current evidence | Disposition |
| --- | --- | --- |
| Numeric decoding copies | `engine/src/de.rs`, `num.rs`; #1358 `722e878afb8` | Shared helpers landed. Preserve selected strict/lenient policies. |
| Socket supervision | `engine/src/socket.rs`; #1350 `d17518bc5f5`, #1385 `6515470afe8`, #1406 `a20f8756519`, #1423 `ed60886b0ba`, #1433 `e923dffa5fc`, #1512 `b49b6f7a236` | Shared supervision/specs landed; protocol authentication remains venue-specific. |
| Wasm-only registry | `registry/src`, both host factories; #1467 `193077a0ae1`, #1469 `b1f093d2624`, #1476 `0741419bf20`, #1519 `93638524384`, #1520 `418e4f6ebc4` | Shared registry/options/host-clock policy landed. |
| Bybit model and venue-named features | `engine/src/model.rs` has `BybitBbo`, `bybit_bbo`, tick tolerance; `engine/Cargo.toml` has `command-asterdex`, `command-apexomni`, `command-polymarket`, `command-kucoinspot` | Coupling debt only; owner deferred pure cleanup until after cutover. Preserve JSON/semantics in any later change. `command-apexomni` has no Rust cfg consumer; do not infer behavior from feature names. |
| Ambient random client IDs | `engine/src/entropy.rs`, `Cx` hooks, host factories, bans; #1624 `0cd8f2b1657` | Explicit identity/signing entropy landed, with typed construction refusal and production cryptographic defaults. |
| URL encoders | `engine/src/url.rs`; #1368 `e4cdde22cd3` | Shared dialects landed; signed payload spellings remain pinned. |
| Paging / book stitching | `engine/src/paging.rs`, `book.rs`, `RUST_HISTORY_PAGING.md` | Paging helpers landed with documented exceptions. Shared level storage does not eliminate venue-specific snapshot/sequence/checksum contracts; no production defect follows from their count alone. |
| Read-key selection / copied probes | `credentials.rs` shares DTO/validation/redaction/erasure only; Binance `signed_request`, KuCoin spot `request`, BloFin `headers`, Bitget `signed_headers` still choose credentials. #1427 `1435005aea3` supplies shared `ProbeVenue` | Selection is duplicated, but no wrong-key defect was established; owner deferred pure deduplication until after cutover. Preserve primary-only exceptions and venue signing. Thirteen secondary-test files keep independent assertions; the copied probe implementation is stale. |
| Documentation and roster | Retained evidence plus `README.md`; roster PRs #1309 `3acf9e46392`, #1310 `45d7a3fb988`, #1312 `373cc098643`, #1318 `f66bdc26f9a`, #1319 `31ec401f601`, #1393 `cb371229da1` | Owner decided index, not consolidation. #1344 closed unmerged; do not label its entire performance proposal landed. |
| Manifest/scripts/comments | `sdk-rs/Cargo.toml`, root `package.json`, `.cargo/config.toml` | All 68 Rust scripts' 61 source paths/patterns resolve; 128 KiB matches 131072. No remaining broken-path/comment defect demonstrated. Feature-name debt remains separately open. |

## Gate honesty evidence

The original enforcement work merged as #1346 (`a22e6b6ac1e`). Current source
is authoritative over intermediate measurements in its historical progress log.

| Finding | Current evidence | Disposition / limit |
| --- | --- | --- |
| Circular margin expectations | Isolated `ts-oracle.mjs`, fixer AST/integrity controls | Tape-native Binance/Bybit expectations; no candidate access. Existing tests verify hostile accesses fail. |
| Omitted financial/order fields, duplicate masking, final-only comparison | `comparison-policy.mjs`, raw checkpoints, independent boundary/presence controls | Fields default on and duplicates fail before maps/fixers. Null/null remains missing evidence, not a capability proof. |
| Unbounded fixers | `fixer-contracts.mjs`, mutation observers, legacy detached oracle controls | Exact venue/leaf permissions and overwrite counts; candidate requests remain immutable. Some TS sections are still corrected. |
| Three-run / partial-load timing | `performance-policy.mjs`, audited/performance mode receipt checks; #1461 `dfbc0a94bc2` | Seven minimum pairs; nine for sub-100 ms main/steady windows with max(1 ms, 5%) allowance. Larger CPU windows and all process CPU/memory keep 10%. Small-window sensitivity is explicitly limited. |
| Missing combined budget / layout | `wasm-budget.mjs`, workflow before standalone overwrite; #1393 `cb371229da1`, #1528 `c101603d0c0`, #1535 `9235412ef74` | One source: 1343488 B family / 14155776 B combined. Ordinary family timing is not combined timing; explicit combined dispatch exists. |
| Missing Bitget USDC gate | Exact `ci-gate-suites.mjs` entries and synthetic fixture provenance | Mandatory public/private cases exist. Incomplete commands remain FAIL, not parity. |
| Nondeterministic private clocks / no live-private corpus | `polling.mjs`, catalog, `HARNESS_TAPE_TIME.md`; merged sweep below | Effective clocks are deterministic for 50 aliases. Five-pair raw equality proves per-implementation repeatability, not private parity, independent protocol validation or live behavior. Approved all-venue live evidence is still absent. |
| Rebuilt TS sections | Isolated fixers and exact leaf declarations | Still a limitation; raw checkpoints remain uncorrected and overwrite counts visible. Do not strike the limitation because clocks are deterministic. |
| Paired TS fixes move the oracle | Production TS implementations plus retained executable references | Owner decided to retain executable TS CPU/memory references through cutover. No TS/reference deletion; later retirement needs independent correctness/performance evidence and declared deltas. No frozen-output completion claim. |
| Rust-driven public fixtures | Existing `record.mjs` constructs Rust; opt-in [public TS recorder](HARNESS_TS_PUBLIC_RECORDER.md) supports audited Binance USD-M with offline transport controls | Coverage remains unproven: no live TS-origin recapture was performed. Other transports/venues require audits; authored synthetic Hyro/Breakout/Bitget-USDC provenance stays explicit. |
| No real browser observations / harness timing scope | #1688 `b6cb612bc7c`, separate browser worker measurement; #1661 `391dbe9af87` artifact evidence | Browser observations now exist. Node replay timing still includes shared replay work and does not measure browser/account startup. No browser threshold; proposal only after three complete landing baselines. |
| Standalone versus combined evidence | Routine workflow family mode; explicit `replay_artifact: combined`; separate browser diagnostic | Each measurement certifies its own artifact/head/workload only. Browser readiness is not a replay CPU/memory or account-startup gate. |

Selected actual sweep receipts: #1532 `cf83f43768a` (shared private clock work),
#1624 `0cd8f2b1657` (per-engine tape-seeded identity), #1622 `227129e7f25`
(complete Bybit IDs), #1663 `e033ec44f2e` (Kraken), #1673 `21a9081c18c`
(Lighter), #1677 `0bc79b6e83a` (Robinhood), #1682 `69d783d94b6` (Bitrue),
#1689 `09f2253a284` (Hyperliquid) and #1692 `598c21db8ec` (Orderly).
Final sweep #1694 `e87107621f6` adds the 50-name effective-clock regression and prunes exactly 20 stale WEEX fee leaves
(132 to 112), retaining the tape hash, other leaves and complete raw-ID comparison.
The full per-family ledger and exact raw-output hashes are in `HARNESS_TAPE_TIME.md`
and the linked PR bodies. Only top-level metrics are excluded; raw client IDs,
transport receipts, commands and checkpoints stay in repeatability comparisons.

## Remaining ownership and decisions

- **SDK/engine disposition:** no remaining production behavior defect was
  established in the Architecture findings. Seventeen venue families repeat
  credential selection, but their existing signing, primary-only exceptions,
  availability rules, passphrase handling and transport routes are intentional.
  The owner deferred this pure deduplication and model/feature-name cleanup until
  after cutover: changing signing across families without demonstrated user
  benefit is unwarranted. No source batch is scheduled by this audit. A future
  demonstrated wrong-key or missing-read-key defect requires its own focused
  regression and review; preserve external JSON, controls and computations.
- **Harness owner (codex-mxb4mq):** ~~Hyperliquid lacks an independent request
  validator, so its proof is repeatability-only.~~ The follow-up
  [independent L1 oracle](HARNESS_HYPERLIQUID_REQUEST_VALIDATION.md) authenticates
  original signatures/nonce/network/vault/subjects and binds generated-ID intent
  before normalization, committing aliases only after eligible ACK matching.
  Published vectors and adversarial Tape controls cover the observed contract;
  full current-head CI/five-pair proof remains the merge gate. Existing private
  account FAILs and synthetic/live evidence limits remain.
  ~~Orderly's unstaged mutation ACKs expose current command snapshots too soon.~~
  The separate [ACK-fence repair](HARNESS_ORDERLY_ACK_FENCES.md) stages twelve
  mutation replies per tape and holds final snapshots until all staged ACKs for
  a command arrive, including two-write stages. Actual-Tape controls cover all
  fourteen identities; stripping only the added stages reproduces the original
  bytes. Settlement remains a scheduling fallback rather than success evidence;
  current-head five-pair artifacts/full CI remain the merge gate.
  #1694 closes the sweep's exact 50-name effective-clock
  assertion and 20 stale WEEX fee keys; its complete ten-output proof and exact
  112-key declaration check do not establish private parity. Coordinate future
  validator/fixture changes with that lane; no harness files changed in this PR.
- **Owner/cutover evidence:** choose approved accounts/environments/minimum-size
  live captures, the read-only 3–7 day production shadow and per-venue canary
  rollout. Scripted lifecycle tooling and green declared-FAIL CI do not provide
  those production observations. No live network/trading/deployment is authorized
  by this documentation audit.
- **Still unavailable evidence:** native live-fill timestamps are not carried
  by the current `Fill`/FILLS contract; optional execution IDs may also be absent.
  Missing fields stay unknown. Reconstructing them from the tape would fabricate
  candidate evidence; a future completeness requirement needs native provenance
  and a separately reviewed contract, not a relaxed comparison.
- **Settled decisions:** retain evidence docs and add the index; retain executable
  TS references through cutover; keep existing budgets and timing policy. Browser
  cold start remains measurement-only. A threshold proposal, never implementation,
  waits for at least three completed real-landing reports with full samples and
  comparable browser/runner provenance. Own-head dispatches and skipped/cancelled
  runs do not satisfy that condition.

## Checks

Source inventory: 32 required suites / 110 exact cases; 50 effective deterministic
private policies; 13 tracked secondary-test modules; 68 Rust scripts and 61
referenced source paths/patterns, all resolving. These counts are tied to the
pinned source, not a blanket live readiness claim. Forty-nine existing focused
enforcement controls pass, including hostile fixer access, protected diagnostic
and workload failures, timing floors and exact budgets. All 152 index paths,
219 local links and the actual merged ancestor pins validate. Independent plan
and documentation/evidence reviews are clean. Refresh the inventory on landing
integration before final CI and the separate guard.
