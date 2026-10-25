# Orderly immediate candle and identity completion custody

## Scope and source checkpoint

Owner codex-5c57jl. Base actual 76f1f7fc842738f83e2f48e7a0a35725eed5f0f3;
branch feat/rust-orderly-local-read-completions. The prior public seed slice
#2097 is landed. This checkpoint contains plan/index/new original tests only.
No engine, signer, credential selection, nonce, physical sender, callback,
account/history/trading reducer or socket change is planned.

Replace six immediate numeric completion occurrences in lib.rs::on_command:

| Native entry branch | Fingerprint | Count |
| --- | --- | --- |
| Candle unknown symbol | b60a1032f327 | 1 |
| Candle unsupported interval | 7d4810ffde4e | 1 |
| Candle invalid parameters | 136ab46ef947 | 1 |
| Candle future lower bound with omitted upper bound: empty result | 2b6c87682e5a | 1 |
| FetchAccount local identity success | c5a021da6919 | 1 |
| FetchAccount local identity failure | c873db6fc77d | 1 |

Current Orderly inventory is 23 fingerprints / 24 occurrences. Later production
removes six / six, leaving 17 / 18, and removes the now-empty lib.rs entry.
Counts describe lexical completions, not physical lifecycle adoption. All maps
and current production stay unchanged in the plan/original checkpoint.

## Native behavior retained

Candles validate symbol before interval before parameters. Parameters reject
zero limit, nonfinite/out-of-domain bounds, and an explicit reversed range.
For a valid lower bound beyond receipt time with no explicit upper bound, native
returns the exact string [] locally. All branches stay before read admission of
accepted candle transport. Preserve clamping, floor-to-seconds, native streamed
current candles versus non-streamed historical candles and both endpoint wires.

FetchAccount returns the exact broker application ID as both userId and
affiliateId when application_id is nonempty and public_data_only is false.
It requires neither a secret nor an HTTP request; public-only, absent and empty
IDs retain Orderly private account ID is required. Do not introduce credential
requirements or a new selection policy. Native JSON escaping remains serde's.

## Command-entry authority contract

Use the existing genuine command receipt, Operation::Read, keyless admission,
Never and no pacing. A bounded local Read policy may use 20 seconds capped by
the original caller receipt, following the actual local-history precedent.
Check the complete CommandWait disposition: None uses shared unstamped
rejection; Expired uses shared expired rejection; Remaining permits admission.
An admission refusal owns its own terminal and receives no numeric fallback.
No copied issuer/ID test, forged root, callback entry or duplicate terminal.

Finish the admitted Context using done_context/fail_context and the unchanged
native result/error. A local Read has no mutation outcome. An unstamped or
expired receipt must refuse even a locally successful identity/empty result;
this is an explicit authority correction, not a change to finite native values.
The helper is callable only in these six immediate on_command branches, never
in physical response, stream, timer, queue, continuation or delayed entry.
A successful native result must not bypass the receipt check. Existing accepted
candle command roots/legs and all response handlers remain byte-exact.

Local Read finalization can emit shared managed timer arm/clear bookkeeping.
Tests permit only matched same-command timer cleanup, never HTTP, managed HTTP,
Sign or socket actions, outstanding timers or Store mutation. Finite native
errors keep their original kind/text/outcome; native success keeps exact JSON.

## Original controls and source gates

A new dedicated local_read_completions target uses genuine Engine command
entry and complete action-batch inspection. Independent unstamped controls
cover the three candle failures and both identity result arms. Native working
controls pin validation precedence, invalid bound variants, exact empty result,
identity metadata/escaping/public/missing/empty IDs and accepted current/history
wire. Bootstrapping completes genuine managed Markets and Tickers first.
Do not describe these legacy originals as capacity or duplicate-owner proof.

Before source freeze add real admission-pressure/recovery, retained-owner
rejection, explicit expired receipt and fixed-clock ID/credit reuse controls for
local failure AND success. In particular pin empty result and identity success
under pressure so done_context cannot be bypassed. Preserve every original
assertion and financial value. Any necessary collector adaptation must use
actual Prepare/Bind/Claim and be disclosed. No production before bounded
plan/original review CLEAR; then standing approval permits production and draft
CI in parallel with source reviews. Auditor exact-tree GO remains mandatory.

Only scoped local builds, CARGO_BUILD_JOBS=4, one at a time. Original compile
failures are not RED evidence. Record assertion failures separately from working
controls and keep logs outside Git. No threshold or generic engine change.

## Original execution receipt

Plan commit b61a07f0314 precedes original test commit 87c21b39da6. On unchanged
actual production, the dedicated target compiled and executed 11 controls:
5 intended assertion RED, 6 GREEN, zero ignored. No compiler/setup failure.
The three candle refusal REDs and missing-identity RED reach native kind None
versus expected InvalidInput. The identity-success RED reaches its missing
failure assertion because legacy emits CommandDone; its later kind/outcome
checks remain source-stage gates. Every complete-batch/Store assertion before
these boundaries passed. Native positive loops retain all documented variants;
they are six passing test bodies, not a claim of independent RED variants.
Scoped target strict Clippy and package fmt pass. Owner logs and full-tree proof
are under /tmp/codex-orderly-local-read-original* and are not repository files.
Only index, this plan and the new target differ from actual76f; all production,
old fixtures, Cargo, maps and checkers are byte-exact. No push or CI at this
checkpoint; bounded native/custody review is required before production.

## Additional original admission controls

While reviews were unreachable, added a separate local_read_admission target on
unchanged production. Its forwarding Probe retains native capabilities and
streamed item handling, bootstraps genuine Markets/Tickers where needed and
allocates 256 genuine Background peers in a preceding no-ID callback.
Six independent capacity controls cover all six branches, including empty
candle and identity success. Each reaches the intended missing shared-capacity
assertion: legacy emits its native result/error instead of one shared refusal.
The recovery assertions behind those RED boundaries remain cutover gates.
Two working controls prove real 256-slot saturation/recovery and retained
identity owner duplicate rejection, owner completion, 600 fixed-clock native
identity queries and recovery of all credits. These legacy controls alone do
not certify the future helper's admission or credit behavior.

Corrected run: eight executed, six intended assertion RED / two GREEN / zero
ignored. The first fixture run omitted capabilities/on_item forwarding and
failed before intended custody checks; it is excluded from RED evidence. The
first eleven-test target remains byte-exact to its original commit. Production,
old fixtures and maps are unchanged. No source cutover or CI claim follows.

## Refreshed actual-tip original receipt

Plain-merged actual 9d4beba669377ba41908ebb43431a05d10ac8949 into the
checkpoint at 29d0d3f537b. The sole conflict was the index: the result retains
the entire incoming file followed by the exact own row once. All production,
old fixtures, Cargo and inventories are incoming-exact. Orderly source is also
unchanged from the original actual76f checkpoint; the count remains 23F/24O.
Both original targets are byte-exact to 4573999b6dd.

Fresh execution: local_read_admission 6 assertion RED / 2 GREEN, followed by
local_read_completions 5 assertion RED / 6 GREEN; 19 executed, zero ignored,
no compile/setup failure. Cargo stops after the first failing target, so the
second target was executed separately. The RED boundaries are unchanged.
Scoped strict Clippy for both targets and package formatting pass, sequential
CARGO_BUILD_JOBS=4. Logs/proof remain outside Git under
/tmp/codex-orderly-local-read-refresh. Explicit expired-entry and all deeper
recovery suffixes remain source-stage gates, not new original proof.

Renewed native/caller and shared custody review is required before production.
The approved Bitget opt-s follow-up takes priority immediately after actual
#2122 lands; this local checkpoint can be resumed without rewriting history.

## Production source checkpoint

After both bounded plan/original CLEARs at 1115420b89e, the six immediate
branches use one local helper: full command-wait disposition, keyless Read
admission, then done_context/fail_context with the native result/error. The
sole runtime delta is lib.rs. Reversing the original reviewed helper preview
reconstructs the incoming lib exactly; accepted candle transport and every
callback remain unchanged. The inventory prunes only the six listed count-one
keys, leaving 17F/18O.

All 19 original test bodies and 57 original assertion invocations are token
exact. The admission Probe gained an expiry-only setup branch and three source
controls: one local failure, empty success and identity success. A genuine
handle_command entry carries its original five-millisecond receipt; only the
fixture clock advances from 100 to its exact deadline 105 before native entry
forwarding. Recovery is a fresh entry at 110, followed by all-256 credit reuse.
No issuer, owner or receipt is fabricated. The first new empty-range fixture
used from=101, which ceased to be future after the clock advance; it was
corrected to 106 (and 111 for recovery), retaining the exact no-I/O assertion.
No original assertion changed and no baseline RED claim is made for these
three added source controls.

Source controls: 22 PASS, zero ignored. Full scoped native source run: 263 PASS,
zero ignored; after the final new-fixture monotonic recovery adjustment, the
22 affected controls and strict all-target Clippy/fmt pass again. Optimized
Orderly web/node WASM is 956,909 bytes versus the same-source baseline 956,661:
+248 bytes, within 1,343,488. No local CPU gate ran. Original logs, source and
assertion proofs and both size artifacts are outside Git under
/tmp/codex-orderly-local-read-refresh. The first bans check sees a newer foreign
Backpack inventory shrink on origin; merge actual tip before freeze and rerun.
No source review, CI or landing clearance is implied by these owner results.

The freeze preparation plain-merges actual Backpack landing c6a5c4bb0a4 at
25946e19d9e. Only the index needed manual resolution, retaining full incoming
bytes plus the exact own row once. No Orderly/shared dependency or Cargo source
changed in the incoming landing, so the paired size and scoped runtime evidence
carry by exact blobs. Fresh full bans and ratchets pass; the stale foreign
inventory failure is closed without changing any foreign key. Six own paths
remain versus the actual tip; all 5,959 unowned paths are incoming-exact.
