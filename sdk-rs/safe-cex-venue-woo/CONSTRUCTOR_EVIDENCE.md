# WOO authored constructor

Gate SHA: `7733bd1fa84b2743340e5005b27cabe46c1424ff`.
Branch: `feat/woo-class-construction`.
Parent: `3faaeee110930c0f6186e1d571453c3b3e5cc0d5`.
Integration: `1a43f3a8f25b7e5fe1c238794e79bad11e4c4788`, already merged down;
the explicit merge before push reported already up to date.

The parent carries unmerged WOO PRs #7858, #7862, #7867, #7876, #7880 and #7883.
Kucoin response repair PR #7889 is separate and is not an ancestor. This receipt
does not reuse any earlier WOO gate measurements for the new constructor.

One row moves pending to ported: WOO **221 ported / 308 divergent / 3 pending**.
The three class declarations remain pending. The native body maps only
WOOXExchange.constructor, with the separately declared method/domain costs intact.

## Body and scope

Rust owns the authored constructor's sequence: SuperCall, retained three original
arguments, secondary-key client factory and captured API, nested primary rate
limits, destructuring three clients with iterator close before assignments,
proxy-required client, public then private socket, and fill listener. Two live
options reads distinguish the getTransport input from requireProxy input. Call
receivers, factory/client/socket identities and thrown sentinels are native
assertions before maintained-source comparisons.

ConstructionOps supplies standard language operations, including binding the
super-return receiver and instance-element initialization, construction, and
property assignment. The actual TypeScript constructor body is extracted into
a derived class; authored instance-field initializers and complete class-member
assembly are outside that body fixture and remain pending. Reflective realm and
arbitrary altered-superclass semantics are not established by these cases.

Canonical declarations are referenced rather than duplicated:
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#closed-typed-output-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias`,
`sdk-rs/parity/MANIFEST-SPEC.md#numeric-ingress-defect`,
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`, and
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`.

## Measurements

| Gate at the gate SHA | Exit and observed result |
| --- | --- |
| cargo check, WOO, all targets, jobs 2 | 0 |
| cargo test --locked, WOO, class_construction target, jobs 2, one test thread | 0; one group with thirteen actual-source cases |
| parity:extract:check | 0; Would update 0 manifests, 0 added, 0 orphaned symbols |
| node --test --test-name-pattern=WOO scripts/rust-ci-wiring.test.mjs | 0; two passed |

Cargo invocations were sequential; the TypeScript compiler environment was
supplied. The reconciliation writer reported zero additions and zero orphans.
The mandatory WOO command and its exact wiring assertion append the constructor
target, retaining all prior commands and targets. No CI rerun was requested.

The first pushed test SHA `6252e80da0f84446c17e395360335e8fd87efe76` failed scoped
check with exit 101: the test sentinel needed an explicit Rc<String> type.
No later gates ran at that SHA. The table measures the corrected pushed SHA.

Enumeration floor: thirteen modes, ending at on:fill failure. Native fixtures
assert the three original super arguments, active/newTarget identity, exact
client/iterable/socket/handler identities, callback name/arity, inner-to-outer
limiter result flow, rate constants/property order, live options, ordered calls,
and the original error identity. The expected successful trace has 25 literal
effects. Early errors stop at independently expected literal prefixes.

Sub-agent review found three blind spots: an identity rate-limit stub hid result
flow, a binding stub substituted captured clients, and the shared function
primitive discarded name/arity. Distinct inner/outer handles, exact factory
iterable identity and native metadata assertions repair them. The reviewer found
no remaining static constructor mismatch and ran no gates.

Four isolated controls compiled and exited 101 at native assertions before the source oracle:

| Mutation | Observed failure |
| --- | --- |
| Inner maxRequests 4 to 5 | WOO-CONSTRUCTOR native rate option key order |
| Skip second iterator binding value | WOO-CONSTRUCTOR native three iterator reads |
| Swap primary/secondary assignment order | WOO-CONSTRUCTOR native call order |
| Change rateLimit callback arity to two | WOO-CONSTRUCTOR native callback name/arity |

Enumeration ends at callback arity. No survivor in this finite set; no compiler-only failure was counted as a successful control. Exact bytes were restored and the same target passed (exit 0, one group/thirteen cases). The scoped src/tests/scripts preservation diff exited 0 against the gate SHA.

This receipt is the only tracked change after the gate SHA. A full tracked preservation diff excluding this file exited 0; gates were not rerun at the receipt descendant.

Not run: the full WOO oracle target list, workspace compilation, host/wasm clippy,
full monorepo suites, live exchange calls, CI reruns, other venue gates, or full
class declarations/instance-member assembly.
