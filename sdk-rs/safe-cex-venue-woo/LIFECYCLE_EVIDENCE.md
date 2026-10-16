# WOO exchange lifecycle and book traversal

Four pending bodies have native implementations. Parent is
`8ec7e581db3257c7f816f1b92be38e528abbd45c`, carrying unmerged PRs 7858,
7862, 7867 and 7876. Integration base remains
`ac0f5fccf24207b26c279e67da2a88f2c72a7aae`; this is reviewed stacked ancestry.

The native entry points create eagerly polled retained language promises.
Await, promise jobs, non-cancellation and iterator elision are required host
operations. Venue branching, continuations and argument evaluation remain Rust.
Imported journal/fold/orderbook/loop functions are dependencies with independent
mappings and declared domain costs; they are not language primitives. The
journal fixtures isolate body composition using deterministic dependencies.
The book fixture calls the existing native shared numeric update helper.
This does not establish class assembly or unrestricted imported helper parity.

Five native assertion groups cover four startup cases, eight polling cases,
three journal cases and one book case. Sixteen maintained-source observations
stop at the live book fixture. Independent native fields, reference identity,
fixed expected traces, counts, errors and lifecycle effects precede source calls.
The finite promise fixture immediately polls entry tasks, retains sibling
continuations after rejection, and delays fulfillment through fixture jobs.
It is not an exhaustive ECMAScript scheduler/realm implementation.

Local review caught source binding elision and unused eager promise capability;
the binding pattern now distinguishes skipped values and async public wrappers
immediately create retained promises. The measured receipt follows.

Canonical declarations remain in
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

## Measured receipt

Gate SHA `868f703fd348d639daeb3e71d1e5a274f528735c`.
Independent tomllib: 218 ported / 308 divergent / 6 pending, 532 rows in
21 manifests, stopping at woo.ws-public.ts.toml. Four pending→ported against
the parent, 32 cumulatively against integration.

Measured exits:

- Scoped WOO all-targets Cargo check, jobs 2: 0.
- Focused legacy_lifecycle target, jobs 2 / one test thread: 0, five passed.
- Extractor check: 0, 7341 symbols / 512 files, zero updates/additions/orphans.
- WOO-only CI wiring tests: 0, two passed.

Earlier SHA `250b7bca4a` check exited 101 because the test's 3,000,000,000
literal inferred i32; no later gates ran there. SHA `148bf5e994` check passed
0, but focused tests exited 101, two passed / three failed: integer-versus-float
JSON expectations and a cross-realm source object assertion. Native numeric
fields and source book numeric fields now have explicit assertions; these
fixture corrections did not change production. No extractor/wiring was run at
that earlier SHA. These failures were not deliberate sensitivity controls.

Review found missing native polling payload/error assertions; balance and orders
reference identity, exact loaded values and emitted message are now asserted
before source capture. The native scheduler callback separately pins numeric
increment and string concatenation. Eager entry, overlap, post-rejection market
continuation and post-await disposal are observed before source traces.

Eight isolated compiling controls exited 101 at native assertions before source:
startup public socket target, startup binding elision, polling balance identity,
polling orders identity, polling loaded booleans, emitted catch message,
history millisecond default, and book amount index. Each edit was restored
before the next. The first order-payload attempt exited 101 at the compiler's
unused-variable error and is not a gate; the corrected edit retained variable
use and failed the native identity assertion. The first control runner restored
and passed all five after that invalid attempt; the remaining runner finished
with a byte-exact restoration and another five passed, exit 0. The eight tested
semantic mutations did not survive; untried edits are not covered by that claim.

Local sub-agent review independently checked 532-row census, mappings, preserved
exemptions, imported-helper limits, raw gates, native-before-source payload
proofs and portability. Reviewer ran no Cargo/gates.

Not run: workspace compilation, full venue suite, other focused native targets,
host/wasm clippy, wasm compilation, live exchange tests, exhaustive scheduler,
class assembly or unrestricted journal helper parity.
