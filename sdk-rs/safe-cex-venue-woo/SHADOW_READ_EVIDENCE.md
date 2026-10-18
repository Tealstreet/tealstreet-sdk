> Historical evidence: the test-only V3 comparison/canonical bodies, operation holders and their oracle commands are retired. The live legacy operation shape is retained; see CLAUDE.md.

# WOO runtime shadow comparison

Four remaining runtime-selector rows now have native implementations and
representations. The source remains maintained alongside Rust. The comparison
owns decoder validation, catch boundaries and result construction; the opaque
operations supply language primitives, not a host comparison body.

Parent `210e5faf473194aec23f341e0a7b8f483faff125` carries unmerged PRs 7858 and
7862. Integration base remains `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
This stacked ancestry is intentional and is not an integration-only parent.

Four native assertion groups cover immutable declaration carriers, normalization,
twelve comparison fixtures and the Error shape compile constraint. Fourteen
maintained-source cases stop at the surrogate canonical fixture. Native field,
identity, code-unit, order, callback and operation-trace assertions run before
source observations. This is a finite floor, not exhaustive exotic host proof.
The compile fixture includes the production module and stubs only the unrelated
route type to isolate the Error parameter constraint; public route behavior is
not inferred from that fixture.

Sub-agent review found an unrestricted Error parameter. It was replaced with a
live structural Error access contract; a plain structural reference, later name
mutation, optional stack and an arbitrary cause handle are native witnesses.
The source checker resolves the actual Error interface, rather than assuming
its name/message/stack/cause shape. An integer-rejection compile control is
included. Compilation and controls have not yet been measured at this step.

Canonical declarations remain in
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

## Measured gate receipt

Code/gate SHA `8093c688aec193c4f20823d368b82fff3f49e7ca`.
Tomllib census: 206 ported / 308 divergent / 18 pending; four new
pending→ported rows against the stated parent, 20 against integration.

Measured exits:

- Scoped WOO Cargo check, all targets / jobs 2: 0.
- Focused shadow_compare target, jobs 2 / one test thread: 0, four passed.
  The type witness accepted a structural Error wrapper and rejected i32 with
  E0277 identifying ShadowErrorReference.
- Extractor check: 0; 7341 symbols / 512 files, would update 0 / add 0 /
  orphan 0.
- WOO-only CI wiring target: 0, two passed. Mandatory WOO CI includes both
  focused canonical and shadow targets.

A compiling four-family mutation batch exited 101 at four named assertions:
integer Error rejection, observed canonical UTF16, original Error identity,
and authoritative identity. Removing the actual production trait bounds made
the invalid i32 fixture compile, so its rejection assertion failed. Exact
source-byte restoration then exited 0, four passed. This is a bounded combined
batch, not an exhaustive mutation enumeration or a claim about every edit in
isolation.

Sub-agent static review found no remaining mapping or body blocker after the
Error contract correction. The receipt commit preserves every production,
test, oracle, manifest, dependency and CI input byte-for-byte against the gate
SHA. No full venue/workspace suite, clippy, wasm, network or live venue test
was run. The earlier canonical-target results apply to their named SHAs; that
target was not re-run for this independent module addition.
