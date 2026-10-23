# Bitget CPU option-B follow-up

## Base and boundaries

Start from actual landing `b627e1bf6e9857158e41e14a49e1faa05572efc5`
on fresh branch `feat/rust-bitget-cpu-option-b`. This work has no merge window.
The queued snapshot and checksum PRs stay separate.

The current instruction requires plan-first work, measured before/after Bitget
public/private steady replay, unchanged fixtures and gates, and no venue-local
bypass of shared paths. Preserve all CPU thresholds, build arguments, declared
private diagnostics, checksum serialization, signing and transport behavior.
The stale build.sh prose can be corrected without changing its arguments.

The earlier Sam-approved option B means a shared second-runner remeasurement
policy: sub-5% CPU-only eligibility, two normalized ratios averaged, fail closed,
no third attempt. Its parked implementation changes certification/workflow
behavior. That is distinct from a runtime optimization and cannot silently be
reclassified as leaving gates unchanged. Resolve this scope conflict before
source implementation; do not import the parked certification changes or select
a new performance algorithm while the definition is unresolved.

## Measurement plan independent of the scope choice

1. Pin landing Rust run `39953368561` to the exact base above. Its Bitget and
   BitgetUSDC physical jobs are `120413470535` and `120413470400`. Retain their
   complete raw outputs and strict reports, exact artifact IDs/digests, source
   package fingerprints, tools and size budgets when available. A cancelled or
   failed run remains labeled as such; missing evidence is not a passing sample.
2. Preserve the committed public tape and private tape generators byte for byte.
   Cover Bitget public, Classic private, inverse private, USDC public/private.
   Public CI supplies nine interleaved TS/Rust pairs and ordinary medians.
   Existing private replay supplies correctness-audit steady/process CPU only;
   these are informational single samples, not public-style performance gates
   or statistically robust private throughput medians.
3. The after measurement must identify the exact candidate source and compiled
   family package. Record TS/Rust steady and process CPU, sample count/order,
   tools/profile identity, WASM bytes/delta and budget headroom. Keep every
   first result, including misses. Do not attribute runner variation to code.
4. Compare complete canonical nonmetric Rust raw outputs for every selected
   public/private case and sample, removing only top-level `metrics`. Retain all
   nested fields, notices, transport receipts, commands and ordered arrays.
   Compare declared diagnostic identities and full failure/fixer lists as well.
   No fixture changes, exclusions or normalization can make a candidate pass.

## Plan review and implementation gate

Before source edits, review the selected scope, exact shared path, compatibility
controls and the measurement interpretation. Historical attribution identifies
public envelope/ticker decoding and shared JSON numeric/string scanning as hot
work; CRC is absent from the regular public performance tape. Bitget's number
policy already delegates to engine de/num helpers. An optimization must extend
that shared implementation, with all existing consumers covered, rather than
introduce a separate Bitget parser or skip required output.

If the current no-gate-edit boundary selects runtime optimization, identify a
concrete equivalent shared-path change and its observable cost before editing.
Keep strict parsing, duplicate/unknown fields, finite-number policies, decimal
rounding, malformed-message recovery and all wire outputs unchanged. Review and
commit that concrete plan before implementation. Differential tests must cover
its actual semantic risks; a performance hypothesis is not a native bug claim.

If the intended scope remains the approved remeasurement policy, obtain explicit
clarification of the certification changes permitted under the no-gate-edit
instruction, then replace this implementation section with the bounded selected
policy and exact affected files. Do not publish an operational rescue path that
cannot be checked end to end, or import the parked API-budget blocker as solved.

After the selected implementation, run focused native or policy tests as
appropriate, strict fmt/Clippy or harness checks, root typecheck/lint/tests, and
independent correctness/adversarial review. Publish a draft with honest current
measurement evidence and outstanding limitations. No guard or merge without an
assigned window.
