# Private replay tape-time sweep

Landing branch: `feat/rust-rewrite`. Harness changes only; preserve strict
FAIL results for documented TS/Rust differences.

1. Finish the parked OKX family replay: virtual private clock, exact observed
   diagnostic keys bound to each tape SHA, and the post-setLeverage USDT
   margin-in-use rationale at checkpoints 9–11.
2. Move remaining wall-clock private replays to tape time, one venue/family
   per PR in the handoff order. Remove only unobserved declarations; new
   Rust-ahead differences require exact keys, tape SHA and a specific reason.
   Keep Phemex startup time rewinding for signed request validation.
3. Extend `weex-replay-clock.test.mjs` to assert the covered venues.
4. Clarify WEEX client-ID declaration wording to describe TS-null IDs.

For each PR: review the plan and diff, run focused harness checks, require at
least five identical strict runs using Node 22 and fresh record/verify outputs,
review independently, and merge only after green CI. Use CI-built artifacts
or CI's five-pair mode on `wip/harness-tape-time-<suite>` branches. That
branch flag applies only to the matching replay matrix job (the Bitget family
sweep also includes its USDC matrix job); ordinary branches
keep one private pair. Compare all five raw outputs per implementation after
excluding measured metrics and record run ID and output hashes in the PR.
Do not build local release wasm or run public timing sweeps. Merge pinned landing SHAs with porcelain Git and inspect the complete
landing-to-head diff before merging. Report every merge to the overseer.

## OKX verification

The PR body records current-head CI run IDs, tape identities and all raw
artifact hashes. Each of the four family tapes must reproduce its complete
nonmetric output five times per implementation, with no unexpected or
unobserved private declarations, before merge.

## WOO phase

Enable the virtual private clock on WOO only. Use the matching sweep branch
to collect five strict CI pairs, compare every complete raw output except
measured metrics, and narrow only the exact unobserved tape-bound declaration
keys. Inspect order censuses, poll/command ties and snapshot staging in the
artifacts; do not change production behavior or broaden declarations. Run
focused harness tests, independent review, full CI and the fail-closed merge
guard, then report the merge. OKX landed in PR #1557 (`3d73437e589`).

## CI-built engine reuse

Before Binance, land a separate infrastructure PR approved by the overseer.
Archive the three actual CI-built Node wasm package files immediately after
each venue build. Record their hashes, source head/run, a fixed build-input
git-tree fingerprint, family and pinned compiler/tool/flag contract. Upload
this package separately for three days; keep existing replay-report retention.

Add a gate option that validates the fingerprint against the tested HEAD,
refuses dirty or untracked build inputs, checks family/tool/flag provenance
and all package hashes, and passes only the validated JS path to Rust replay.
Any missing or mismatched artifact fails without falling back to installed
wasm. Harness-only clock/declaration edits can reuse unchanged Rust inputs.

Test matching reuse, mismatched commits, dirty/staged/untracked inputs,
root Cargo configuration, family/tool/flag changes, missing/corrupt package
files and early CLI refusal. Review independently, run the focused harness
suite, and require full green CI on the PR head before merge. Local replay
outputs accelerate iteration; they do not replace that merge gate.

## Binance family phase

Enable the shared virtual private clock for Binance USD-M, Spot, inverse and
portfolio margin, including the frozen scripted lifecycle capture. Generate
exactly the same synthetic tapes as CI and keep the frozen capture unchanged.
Use a matching engine from CI for five focused private pairs per identity;
inspect complete outputs and remove only exact unobserved tape-bound keys.
Declare any new Rust-ahead difference only after reviewing its exact keys,
tape SHA and specific cause. Preserve all strict FAIL results.

Run focused harness tests and independent review, then the nine-case Binance
CI suite with five pairs for all five private identities (50 raw outputs).
Record run ID, tape identities and every raw hash, verify fresh CI reports,
require full green head CI and the normal Binance dependency guard, merge
with a separate call and report the receipt before the next family.

## Bitget family phase

Enable the virtual private clock for Bitget USDT-M, inverse and USDC-M.
All three aliases share the Bitget engine. Generate the unchanged CI tapes
and use a fingerprint-matching CI engine for focused private iteration.
Remove only exact unobserved declarations bound to those tapes; inspect any
new diagnostics before adding exact keys, tape SHA and a specific rationale.

The Bitget sweep branch must collect five private pairs in both the `bitget`
and `bitgetusdc` matrix suites, including inverse in the former. Other
branches and suites retain one pair. Require identical complete nonmetric
outputs within each implementation for all three tapes, fresh mandatory
suite verification, independent review, full green head CI and a separate
landing guard before merging and reporting the receipt.

Raw five-run inspection found generated amend client IDs varying in all
three Rust aliases. The initial Bitget-only JavaScript crypto binding hook
was replaced by the explicit host entropy work in PR #1624. Both Rust replay
modes now inject an isolated per-engine identity stream seeded from the exact
tape SHA, preserving the original SHA/domain/counter byte stream. Signing
keeps production cryptographic randomness. Keep complete raw IDs in the
comparison. Native hosts now expose an explicit entropy provider too; see
`RUST_HOST_ENTROPY.md` for that separately owned implementation.

## Blofin phase

Enable Blofin's virtual private clock without changing wire payloads,
protocol validators or production behavior. The first raw proof found that
the tape lacks an epoch and TS live fill rows use Date.now as a fallback.
Add one metadata frame anchored to the authored history window, preserving
all existing wire and command records. Rebind only the active exact-key
declaration to the new tape SHA; preserve the historical declaration.
Use a fingerprint-matching CI engine for five focused private pairs.
Inspect complete outputs, including client IDs, lifecycle checkpoints and
transport receipts; compare all nonmetric fields within each implementation.
Remove only exact unobserved declarations bound to this tape SHA, and review
any new diagnostic before adding exact keys and a specific rationale.

The existing matching sweep-branch flag supplies five private pairs in CI;
normal jobs retain one. Require fresh two-case suite verification, independent
review, full green head CI, a separate landing guard and an audited merge
receipt before reporting the merge and moving to MEXC.

## MEXC phase

Enable MEXC's virtual private clock using the fixture's existing epoch.
Keep wire data, protocol validators, large native identities, auth feedback
and production behavior unchanged. Use a fingerprint-matching CI engine for
five private pairs; compare complete nonmetric outputs, including fills,
settings and transport receipts. Remove only exact unobserved keys from the
active tape-bound declaration after reviewing the raw evidence. Preserve
historical tape entries and public declarations.

Require the existing matching-branch five-pair CI mode, fresh full-suite
verification, independent review, green head CI, a separate landing guard
and audited merge receipt before reporting and moving to Polymarket.

## Polymarket phase

Enable the virtual private clock in Polymarket's separate harness config,
using the fixture's existing fixed epoch and preserving all wire records.
Keep both standard and negative-risk signing-domain validation, holdings
availability, trade-status deduplication and command acknowledgements intact.
Use a fingerprint-matching CI engine for focused private iteration; compare
five complete nonmetric outputs per implementation, including IDs and times.
Only remove exact unobserved tape-bound declarations after inspecting raw
outputs; any new difference needs an exact key and a specific reason.

Require the existing matching-branch five-pair CI mode, fresh mandatory
public/private suite verification, independent review, full green head CI,
a separate landing guard and an audited merge receipt before reporting the
merge and moving to KuCoin Spot.

## KuCoin Spot phase

Enable the virtual private clock in KuCoin Spot's separate harness config,
using its existing fixed epoch without changing wire records or protocol
checks. Preserve bullet-token roles, welcome ordering, signature checks,
caller IDs, staged order snapshots and the final reconciliation wait.
Use a fingerprint-matching CI engine for five focused private pairs and
inspect complete nonmetric outputs, including cumulative fills, cancelled
rows, timestamps, IDs and command receipts. Recheck the handoff's reported
paths against these outputs; declare any observed difference only with
exact keys, tape SHA and a specific cause. Prune only unobserved keys.

Require fresh mandatory public/private suite verification, independent
review, five identical current-head CI pairs, full green head CI, a separate
landing guard protecting the shared `kucoin` Rust crate, and an audited
merge receipt before reporting and moving to BingX.

Five focused pairs reproduced the handoff's 13 additional paths: eight
filled/remaining fields where TS REST replacement restores startup values
after the WS update while Rust preserves the cumulative fill, and five
retained cancelled-row paths. Declare only those exact paths on the existing
tape SHA and remove two unobserved checkpoint-5 row keys; retain strict FAIL.

## BingX phase

Enable BingX's virtual private clock without changing wire records,
protocol checks, generation-aware subscriptions or production behavior.
The fixture currently lacks epoch metadata; change that only if complete
raw evidence demonstrates a timestamp problem. Preserve signed request
validation, listen-key roles, large native IDs, staged position snapshots,
cumulative-fill deduplication and the 750 ms position repair.

Use a fingerprint-matching CI engine for five focused private pairs.
Compare complete nonmetric outputs, including IDs, timestamps and receipts;
prune only exact unobserved tape-bound keys and inspect any new diagnostic
before adding an exact key and specific cause. Require fresh mandatory
public/private verification, independent reviews, five identical head CI
pairs, full green CI, a separate BingX dependency guard and an audited
merge receipt before reporting and moving to the Bybit family.

## Bybit family phase

Enable the virtual private clock in the shared Bybit harness factory for
linear, inverse, Spot, demo and Hyro. Preserve all protocol/signing checks,
environment routing, generation-aware subscriptions and wire payloads.
Start with the clock flag; change fixture metadata or replay entropy only
if complete raw evidence demonstrates a problem and ownership is coordinated.

Coordinate with the Bybit row-starvation fix and its leverage-ACK/position
fence follow-up. Final declarations and proof must use the actual merged
production sources; re-integrate and re-derive if those land during this
phase. Earlier iteration evidence is historical once dependencies change.

Use a fingerprint-matching CI engine for five focused private pairs per
alias. Compare all 50 complete raw outputs excluding only measured metrics,
including generated order-link IDs, timestamps and command receipts. Derive
exact tape-bound declarations from observed outputs; never carry stale keys.
Require fresh ten-case suite verification, independent reviews, five head
CI pairs per private identity, full green CI, a separate guard protecting
the shared `bybit` crate and an audited merge receipt before reporting and
moving to Crypto.com.

## Crypto.com phase

Enable the virtual private clock only in Crypto.com's harness config. Preserve
the wire fixture, signed request checks, economic parameters, RPC/heartbeat
identities and existing TS readiness/frame hooks. Its fixture has no epoch
metadata; add an anchor only if complete raw evidence requires one. Keep the
existing protocol/auth coverage diagnostics unless fresh exact-key evidence
shows a declaration is unobserved; repeatability alone is not authenticated
lifecycle parity.

Use a fingerprint-matching CI engine for focused private iteration if available,
or the matching sweep branch's five-pair CI proof. Compare complete nonmetric
outputs, including IDs, timestamps, command receipts and reconciliation timers.
Review new differences before declaring exact keys, tape SHA and a specific
cause; prune only unobserved keys. Require focused harness tests, fresh public
and private mandatory verification, independent current-head artifact review,
full green CI, a separate Crypto.com dependency guard and an audited merge
receipt before reporting and moving to GRVT.

## GRVT phase

Enable GRVT's virtual private clock using its existing fixed epoch without
changing wire records, session routing or EIP-712 verification. Preserve caller
identity across the scripted 401/session rotation and resend, native execution
IDs, duplicate-fill handling, leverage refresh and symbol-scoped cancellation.
Keep signature verification before existing request canonicalization.

Validate the prefetched CI engine fingerprint before focused five-pair private
iteration; refuse mismatches without fallback. Compare complete nonmetric
outputs, including IDs, times, command receipts and staged account state. Prune
only exact unobserved tape-bound declaration keys, and review any new diagnostic
before adding its exact key, tape SHA and specific cause. Preserve strict FAIL.
Require focused harness tests, fresh public/private mandatory verification,
independent reviews and five identical current-head CI pairs. Merge only after
full green CI, a fresh separate GRVT dependency guard and an audited merge
receipt, then report and move to Phemex with its startup rewind preserved.

Five focused pairs with a matching CI engine reproduced identical complete
nonmetric outputs. At checkpoint 7 both implementations expose the same two
order rows; remove only the five old declarations for that checkpoint's row
count, row identity, symbol and client ID. Keep the other 33 exact private
keys, strict FAIL, unchanged fixture and existing signature/session validation.

## Phemex phase

Enable only Phemex's virtual private clock. Keep its startup clock rewinding:
leave `monotonicReplayClock` unset, because continuous startup breaks signed
request validation. Preserve expiry freshness, bootstrap auth/subscription/
snapshot ordering, the 1600 ms REST snapshot, terminal reconciliation, duplicate
fill handling and both USDT/USDC account views. Do not change wire records,
protocol validators or production behavior. The fixture currently has no epoch
metadata; add an anchor only if complete raw evidence requires one.

Use a hard fingerprint-matching CI engine for five focused private pairs.
Compare complete nonmetric outputs including IDs, timestamps, settings and
command receipts. Prune only exact unobserved private declaration keys; any
new difference requires reviewed exact keys, tape SHA and a specific cause.
Preserve existing public declarations and strict FAIL. Require focused tests,
independent reviews, fresh mandatory public/private verification, five identical
current-head CI pairs and full green checks. Run the Phemex dependency guard
separately from the merge, audit actual parents/files, report the merge, and
move to standalone Hyperliquid Spot.

Five focused pairs with startup rewind preserved produced identical complete
nonmetric outputs within each implementation. All 33 private keys remain exact,
with zero unexpected or unobserved keys. Keep the fixture, absent epoch metadata
and both private/public declaration entries unchanged.

## Standalone Hyperliquid Spot phase

Enable the virtual private clock only in the standalone Hyperliquid Spot
configuration, using the fixture's existing epoch. Keep L1 action signature
validation, nonce freshness, spot asset indices, caller cloid correlation,
snapshot/live fill deduplication and balance refresh receipts unchanged. The
embedded Hyperliquid spot tape is a separate identity for the later Hyperliquid
phase; do not claim coverage for it here.

Validate an authentic CI engine fingerprint before focused five-pair iteration;
refuse mismatches without fallback. Compare complete nonmetric outputs including
IDs, timestamps and command receipts. Remove only exact unobserved tape-bound
keys, and review any new key with its tape SHA and specific cause. Require
focused harness tests, independent plan/implementation/current-CI raw reviews,
fresh mandatory public/private verification, five identical current-head CI
pairs and full green checks. Guard the shared Hyperliquid crate separately
from merging, audit actual parents/files, report the merge, then move to Kraken.

Five focused private pairs with a fingerprint-matching CI engine are identical
within each implementation, including complete IDs, timestamps and receipts.
The standalone private strict gate passes with zero diagnostics; keep the
17-record fixture and declaration manifest unchanged. Current-head CI must
independently reproduce the proof and verify both mandatory cases before merge.

## Kraken futures phase

Enable the private tape clock only for Kraken futures, using the existing
1700000000000 epoch. Keep startup rewind and the strict Rust increasing/fresh
nonce audit, exact encoded-form/path signatures and TS's existing audit policy.
Preserve reconnect challenge/subscription authentication, fill deduplication,
the staged 3800 ms REST reconciliation, leverage, terminal orders and unsupported
command receipts. Kraken Spot is a separate completed phase.

Use a hard fingerprint-matching CI engine for five focused pairs. Compare full
nonmetric outputs, including IDs, timestamps and command receipts. Remove only
exact unobserved private declaration keys; review any new key with its tape SHA
and specific cause. Keep public declarations unchanged. Require focused tests,
independent plan/implementation/current-CI raw audits, fresh mandatory public/
private verification, five identical current-head pairs and full green CI.
Integrate any actual shared-profile dependency landing before final proof. Guard
Kraken separately from merging, audit actual parents/files, report, then move to
Lighter mainnet/testnet before its Robinhood alias.

Five focused pairs with a matching CI engine produce identical complete
nonmetric outputs within each implementation. Prune only 33 unobserved private
keys, reducing 193 to 160: ten time-in-force keys for order 201 now match, and
23 client-ID child keys are unobserved. Retain the two observed generated-client-ID
keys and all existing parent-row differences. The fixture and all public keys
remain unchanged; strict FAIL and the remaining 160 exact private keys remain.

## Lighter phase

Enable tape time only for Lighter mainnet and testnet through the shared
factory; stage Lighter-Robinhood in its own following PR. Preserve the existing
epoch, independent Go-WASM authentication and transaction verification,
nonces, expiry, deployment domains and mainnet integrator attribution.
Generate the exact CI fixtures and use only a fingerprint-matching CI engine
for five focused private pairs per tape. Compare all twenty complete outputs
except measured metrics, including full IDs, signatures, receipts and state.
Remove only exact unobserved declarations bound to the selected tape SHAs;
review any new diagnostic and its specific cause without broadening coverage.

Review the diff and raw outputs independently, run focused harness tests, then
require current-head CI with five pairs on both private calls, three mandatory
cases, zero unexpected or unused private declarations and all checks green.
Run the dependency guard separately from merge, audit the actual merge and
report its pin before starting Lighter-Robinhood. Keep fixtures, validators,
production code, entropy and the ordinary one-pair CI default unchanged.

Tape-time TS entry/TP/SL generation exposed the production generator's
100-suffix-per-millisecond collision space. A controlled constant-random
replay reproduces an acknowledged ID reuse and the independent verifier
correctly rejects it. The overseer approved a harness-only, domain-separated
SHA-256 stream seeded from the exact tape SHA for private Lighter TS replay,
installed after crypto initialization. Keep real signing crypto, production
ID arithmetic and the duplicate-ID verifier unchanged. The regression runs
the real TS replay under colliding ambient randomness and checks the actual
installed stream and all bracket acknowledgements; the clock-only version
fails it. Native Rust seeds a stateful counter once through Cx entropy and
increments for later IDs; the separate production TS risk was reported.

Remove only four unobserved keys: checkpoint-0 total/upnl on each private
tape (13 to 11 each). Preserve public and Robinhood declarations, fixture
bytes, independent signatures and strict FAIL for remaining differences.

## Lighter-Robinhood phase

Extend the shared Lighter tape-clock flag and the already-approved private TS
replay random source to Robinhood. Keep the entropy algorithm, tape-SHA seed,
post-crypto installation, signature verifier and existing canonicalization
unchanged. Extend the real collision regression to the Robinhood deployment;
the prior Lighter-only initializer must fail that case. Production stays intact.

Generate Robinhood mainnet and testnet exactly as CI, including its shared
testnet filename. Preserve epoch 1792454400000, mainnet domain 466324, testnet
domain 300 and absence of Lighter integrator attribution. Use a matching CI
engine for five private pairs per tape, compare all twenty complete nonmetric
outputs and prune only exact unobserved tape-bound declarations. Require
independent reviews, focused harness tests, current-head CI proof and mandatory
three-case verification with zero unexpected/unused private declarations.
Merge only on full green CI with a fresh separate dependency guard, then audit
and report the actual merge before Bitrue. Ordinary CI still runs one pair.
