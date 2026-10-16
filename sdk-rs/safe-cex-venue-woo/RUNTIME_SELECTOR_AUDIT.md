# Inert runtime route selection

The actual-source oracle landed at `2149de2921` before the Rust implementation.
It executes the production selector and contract module, once per corpus, with
26 routes and six unknown string IDs crossed with 16 modes: 512 observations.
Eight standalone targets cover both generations and all four declared methods.
The initial capture is darwin/arm64, Node v24.20.0; each live invocation records
platform, architecture and full Node version. No generated corpus is committed.

A labeled post-capture finite-payload supplement adds mode 1.25, taking the
selection corpus to 544. It makes no retroactive source-first independence claim.
Numbers are observed before serialization; only NaN is canonicalized. Negative
zero, negative underflow, overflow and nested invalid-mode numeric payloads stay
bit-exact. Native output fields are inspected directly without a roundtrip.

## Boundaries and the earlier-row finding

The three operations reuse the existing route inventory, HTTP methods and gate
table. There is no transport, Store mutation, timer, gate override or migration
activation. Unknown route lookup precedes mode validation, which precedes the
lifecycle checks. Ownerless routes stay blocked; already-V3 routes force GET and
reject shadow mode; shadow writes are blocked. Every canonical migration gate
remains false. The typed output declarations and comparator remain pending.

The source registry is not frozen: requireRoute returns the exported record by
identity, and changing legacyPath changes subsequent selections. Rust returns an
immutable static borrow. The existing WOO_V3_MIGRATION_ROUTES row therefore moves
from ported to CHOSEN divergent. This is a deliberate static-inventory choice,
not a claim that Rust cannot model a mutable registry. The source alias control
changes requireRoute to return a copy and fails `source requireRoute returns
registry alias`; Rust retains the original path in its corresponding witness.

Source plans and targets are frozen; Rust JSON plans are mutable. Source invalid
object modes remain aliased after outer freezing; Rust clones them. These are
separate choices, witnessed independently. String route IDs and JSON modes omit
non-string IDs, present undefined, native NaN and executable objects; the source
undefined and native-NaN behavior is explicitly observed. Typed HTTP methods also
exclude a source runtime PATCH value. UTF-8 refusals have double-escaped lone
surrogate witnesses in diagnostics, target paths and invalid-mode values, plus
ordinary, astral and whitespace positive strings. These choices do not claim
that a more expressive host-object or readonly output model is impossible.

## Controls and scope

Five production mutations compile and fail the existing target at exit 101:

- Ownerless bypass: `route blocker identity`.
- Already-V3 bypass: `route native field`.
- Closed gate inversion: `route blocker identity`.
- Legacy target path replaced with V3: `route native field markets.authoritative.path`.
- Mode guard bypass: `route blocker identity`.

Two further compiling production mutations fail at exit 101 on `route native
number bits markets.requestedMode`: signed-zero erasure and changing finite 1.25
to 1.5. They pin NaN-only canonicalization without weakening other comparisons.

The actual-source alias mutation fails at exit 1; no signing or runtime module
is stubbed. The corpus does not independently establish behavior under arbitrary
runtime edits of route methods or gates: Rust deliberately has no mutable
registry, and canonical GET-to-GET routes do not distinguish the two method
conjuncts. No mutation coverage is claimed for those unreachable configurations.

All work after `06bd313bb9` is local. PR 7274 and PR 7252 stay pinned. No CI,
browser-wasm, clean-install or full merged-state readiness claim is made.

## Repaired integration base and review

`cd5bc80750a401ec5f9d1630d502183eac0447e3` merges integration
`e53d89eddeef1fc1dddf919fed994d0c8b9df48b`. All 107 incoming files match that
parent exactly; WOO source, fixtures and manifests are unchanged by the merge.
Named CI steps retain both parents' multiplicities, 90/90 to 90. A real TOML
parse reports 359 exemption blocks and 359 distinct targets, with eight WOO
dependencies retained and layer containers unchanged.

On that merged SHA, the selector target passes 2/2. Strict host and wasm Clippy
pass separately for the WOO library and runtime_selector test target. Parity
passes at 7,339 rows / 3,377 public Rust items. These package-scoped checks do not
cover MEXC or claim a wasm workspace build. The seven mutation results above
precede the merge; unchanged WOO files preserve their scope, not a replay claim.

The adversarial sub-agent returned MERGE for the selector and integration
extension at that exact SHA, independently checking incoming files, step
multiplicities, dependencies, real TOML parsing and layer containers. It read
the saved test/lint logs and ran no fresh Cargo commands. This is not a full
merge-up gate sheet. Counts including WOO's journal rows are 91 ported,
33 divergent and 410 pending of 534.
