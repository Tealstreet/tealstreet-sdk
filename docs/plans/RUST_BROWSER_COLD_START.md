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
browser library dependency, timing threshold or production behavior changes.
`raw.json` retains all observations, version/source/runner provenance and
failed-sample diagnostics; `table.md` contains medians. The first actual table
and the run/artifact link are recorded in the PR before merge.

Worker bootstrap and end-to-end include the DedicatedWorker launch (parent
clock); module readiness includes production loader import, family import,
WASM fetch/compile/instantiate (worker clock). Chrome launch is separate.
Combined web bypasses the family loader and is explicitly diagnostic. There
is no account construction or exchange I/O. Fresh profiles clear browser
caches; the runner's OS caches may be warm. Three samples are descriptive,
not evidence for a timing threshold or real-user latency distribution.

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
