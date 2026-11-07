# Actual browser WASM cold-start evidence

Base: KuCoin Spot profile repair #1684, d510b981323. Existing family-measure
uses fresh Node require; family integration tests import web glue under Node.
Neither measures a real browser. Production browser/worker loaders choose one
of 28 families. Combined web is shipped for direct/embedded consumers and will
be measured separately as a diagnostic, not as the family-loader path.

Run `corepack yarn rust:measure-browser --inputs INPUTS --out OUTPUT` in CI.
The job downloads `browser-wasm-families` and `browser-wasm-combined` from
the same workflow run, checks source/run/build-input fingerprints and SHA-256
for every JS/WASM file, then starts Chrome from the runner image. No build,
browser library dependency, timing-failure gate or production behavior changes.
The follow-up below adds informational median warnings only.
`raw.json` retains all observations, version/source/runner provenance and
failed-sample diagnostics; `table.md` contains medians. The first actual table
and the run/artifact link are recorded in the PR before merge.

Worker bootstrap and end-to-end include the DedicatedWorker launch (parent
clock); module readiness includes production loader import, family import,
WASM fetch/compile/instantiate (worker clock). Chrome launch is separate.
Combined web bypasses the family loader and is explicitly diagnostic. There
is no account construction or exchange I/O. Fresh profiles clear browser
caches; the runner's OS caches may be warm. Three samples are descriptive,
not evidence for a hard timing gate or real-user latency distribution.

1. Add a measurement harness using CI's existing Chrome and Node 22 built-ins,
   with no dependency or production/WASM code changes. Each of three samples
   uses a fresh Chrome process/profile and real DedicatedWorker. Start the
   worker's module timer before dynamically importing the shipped loader and
   await production loadEngine/default initialization. Measure worker bootstrap
   and end-to-end readiness using each clock only within its own realm.
2. Serve only required public JS/WASM and worker bootstrap assets over loopback,
   with correct MIME and no-store. Validate exactly one expected WASM request,
   hashes, all 28 families plus combined, exactly three complete samples and
   finite nonnegative measurements. Handle CDP errors, exceptionDetails, socket
   closure, worker errors and bounded timeouts. Retain failure diagnostics.
3. Add one SEPARATE CI job only on landing pushes and workflow_dispatch, never
   PR events. Reuse same-run family web artifacts and combined web bytes archived
   immediately after the Binance combined build, before standalone overwrite.
   Verify source/build fingerprints and asset hashes before measuring. Archive
   raw samples, medians, browser/runner/source provenance and a readable table.
4. Measurement only: fail missing assets, errors and incomplete samples, with NO
   timing threshold until several landing baselines exist. Observations describe
   fresh Chrome profiles on loopback; OS file caches may be warm. They establish
   module load/compile/instantiation readiness, not device/network performance,
   exchange startup/account I/O or deployed NX adoption. No local timing matrix.
5. Unit regressions cover malformed/incomplete samples, wrong/missing assets,
   safe serving and CDP failure/wait handling. CI runs the real browser. Dispatch
   the feature head, inspect every sample and post the FIRST actual results table
   in the PR before merge. Update Cutover audit evidence while preserving the
   executable TS performance oracles and all remaining caller-owned work.

Independent plan review is clean with the constraints above. Commit this plan
first, implement and audit, run focused checks, post independent review, then
require final CI, a fresh separate revert guard and exact merge-head/parent/tree
verification. Report actual merge to the overseer and caller/harness lanes.
No releases, deployments, budget increase, new timing gate or NX source edits.

## Warning-only follow-up plan

The overseer approved informational warnings after reviewing three complete
landing reports: 39689379385 (`b6cb612bc7c`), 39691096782 (`850bc25e51e`)
and 39693216736 (`90df20d2f14`). Each retained 29 assets and 87 samples.
Their largest family medians were 28.1, 28.2 and 42.4 ms; combined medians were
51.3, 51.5 and 68.0 ms. Runner CPU, image and Chrome/JS versions differed,
so these are descriptive cohorts, not a device latency distribution or SLO.
A 159.2 ms individual sample had a 22.6 ms median and is retained as evidence.

1. Reuse existing measurement validation and median calculation. Extract the
   completion/table publication path into `completeMeasurements`, called after
   all samples. Add serializable warning records and GitHub warning annotations
   for unrounded `moduleReadyMs` medians strictly above 75 ms for each family or
   125 ms for combined. A valid slow report still completes successfully; no
   individual sample, bootstrap, browser-launch or end-to-end timing threshold.
2. Include source/run and runner platform, architecture, CPU, image and Node,
   plus every observed Chrome/JS version pair for that artifact. Escape workflow
   annotation data; retain raw samples and outliers. Preserve missing-assets,
   wrong-WASM, invalid-timer, error and incomplete-sample failures.
3. Regress the actual completion path: slow success, exact and just-above
   thresholds, the separate combined threshold, a harmless isolated outlier,
   complete cohort provenance, annotation safety and invalid-report failures.
   Demonstrate RED against the prior publication, then GREEN. No new dependency,
   production loader, Rust, budget or replay policy changes. Correct only the
   existing workflow comment; keep jobs, triggers and steps unchanged.
4. Run focused browser/CDP controls, root checks, independent correctness and
   adversarial review, normal CI and an own-head dispatch with all 29 assets /
   87 samples. Post the dispatch table and provenance. Guard the exact four-file
   delta separately at fresh landing, merge the green head and report the pin.

These warnings never fail the job on latency. Revisit a hard gate only after
**ten additional complete landing reports** beyond the three above, with runner
and browser cohorts reviewed; dispatches, skipped and cancelled runs do not
count. A hard timing gate remains a separate decision.

Completed reports carry a `raw.json.warnings` array (empty when no median
exceeds its threshold). Failed reports can omit it if completion is not reached.
Each warning records the artifact, unrounded median, threshold, source/run and
runner/runtime cohort; the same record is emitted as an escaped GitHub warning
annotation. Raw observations and the median table remain intact. Local tests:
all 1,047 harness controls pass, including 16 browser/CDP cases. The five new
regressions fail against the prior publication behavior while the 11 existing
cases pass. Root typecheck/lint and independent correctness/adversarial review
pass; the actual-browser dispatch is reviewed in the PR before merge.
