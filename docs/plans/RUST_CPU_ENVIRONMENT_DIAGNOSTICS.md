# CPU runtime environment mismatch diagnostics

Owner mxb4mq. Base actual1e2458b1c98. Priority follow-up to the shared
two-run CPU certification, ahead of the parked Backpack identity plan.

## Evidence and boundary

Combo5b Rust40093498678 Bybit second physical job120875854314 failed at
cpu-runtime.mjs readCapsule before measuring: environment identity mismatch.
The original cpu-cohort-bybit artifact12559947526 manifest records ImageOS
ubuntu24, ImageVersion20261012.334.1, architecturex64, Nodev22.23.3 and
all process.versions fields. The failed second log records that same displayed
image and Node version. Hosted Compute Agent changed to20261021.618, but
provisioner/agent version is absent from the manifest identity and cannot alone
explain this comparison. No exact differing field is currently logged.

The overseer explicitly narrowed this first PR to diagnostics: retain every
existing identity field and verdict, and do not add a CPU-model binding that
would reject normal hosted-runner hardware variation. A future policy change
requires the actual mismatch evidence; this slice grants no exemption.

## Plan before controls and source

Commit this plan, then assertion RED controls against the unchanged runtime.
Mismatch errors must identify each differing dotted field with recorded and
current values, including nested Node components and added/removed components.
List all differences deterministically, rather than stopping at the first.
Compare only the existing validated environment identity, never raw process.env
or tokens. Keep failure before filesystem restore writes.

Existing same-environment restore/verify controls must remain GREEN with the
same runtime fingerprint. Existing image, architecture, Node and component
mismatches must continue to fail closed. No runner selection, attempt rule,
mean, CPU threshold, replay fixture, build profile, workflow or venue changes.
Only cpu-runtime.mjs, its scoped tests, this plan and nearby CALLERS text are
planned. Source is a deterministic mismatch formatter at the existing rejection.
Run scoped Node runtime/collection/CPU integration tests; full certification
belongs to one fresh explicit Rust CI and one watcher at120s. Independent
bounded source review and normal landing window remain readiness gates.


Original controls: plan7b94d84a6a9 precedes test-only53571745cd3. The five
new controls fail at assertions because the unchanged runtime reports only the
generic mismatch; existing controls remain working. No compiler or import
failure is used as a RED. The source change adds only deterministic recursive
differences to the existing rejection; environment validation remains exact.

Local evidence: unchanged runtime executes24GREEN/5assertionRED/0skip, with
each failure only the missing field-level evidence assertion. After source,
the exact same controls plus CPU collection/CI integration tests pass53/53,
zero skipped. Existing same-identity fingerprints and before-write rejection
controls remain GREEN. No identity schema or comparison policy is changed.

Independent bounded source review at c1ad788661e identified a future-shape
formatter hazard: arrays and objects with the same indexed leaves could compare
as no differences. Current environment validation rejects these shapes. Keep
the original canonical equality as the sole rejection predicate, so diagnostic
formatting can never weaken it, and recurse only across the same array/object
kind. An additional boundary control pins invalid array/object substitutions
as rejected before writes; it is a working control, not an original RED for
an accepted environment. No comparison-policy change is made.
