# WOO socket class bindings

Gate SHA: `441d8699ee6941ff9303d84bdff9f112e1fff2a3`.
Branch: `feat/woo-socket-class-bindings`.
Parent before this slice: `4718fc4b23d9fe0710834140068a413e6b2c8237`.
Integration merged before the gate push: `0445a753756f197e4363222eab9e787868e4f700`.
The branch carries reviewed, unmerged WOO PR ancestors #7858, #7862, #7867,
#7876, #7880, #7883 and #7894. Kucoin response repair #7889 is separate.

Tomllib census: **218 ported / 313 divergent / 1 pending**, of 532 rows.
This slice moves two pending class declarations to divergent CHOSEN and three
previously ported typed heartbeat facades to divergent CHOSEN. No row moves to
ported in this slice. Relative to the original WOO 186/308/38 census, the stack
moves 35 pending to ported, two pending to divergent, and three ported to divergent.
The exchange class declaration remains pending.

Rust now owns the public/private class field layouts and installs callbacks
that invoke the actual corresponding Rust bodies. There is no native-member
factory or string-to-root dispatcher in this assembly. Frames share receiver
state; installed wrappers resolve live methods after parameter destructuring.
The private onOpen arrow retains its async kind and eager task ownership.
Language heartbeat bodies preserve the live pingAt setter and resolved
send/sendWithLog receiver/order. Existing public typed core-socket facade APIs
remain deliberate convenience surfaces; their notes price replaced-member and
setter observations that those facade calls omit.

The class adapter retains owned immutable input for typed child handlers. Public
ticker handlers capture RetainedJsonInput; private timestamps keep JSON ingress. A caller-owned mutable object graph is outside that input surface.
The fifth native group first proves caller row.close becomes 59, then proves the
installed handler reads the earlier sidecar value 31; actual maintained source
reads 59. Both class rows price this lost mutation as CHOSEN. This is evidence of
a boundary cost, not a matching parity assertion. The private container-valued
timestamp restriction is declared but is not independently exercised by that
public ticker witness. No generic mutable payload parity is claimed.

Canonical declarations are referenced rather than duplicated:
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#closed-typed-output-boundary`,
`sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias`,
`sdk-rs/parity/MANIFEST-SPEC.md#numeric-ingress-defect`,
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`, and
`sdk-rs/parity/MANIFEST-SPEC.md#forced-surrogate-conflict`.

## Measurements at the gate SHA

| Gate | Exit and observed result |
| --- | --- |
| Scoped cargo check, WOO all targets, jobs 2 | 0 |
| cargo test --locked, socket_classes target, jobs 2, one test thread | 0; five groups and five maintained-source cases |
| parity:extract:check | 0; Would update 0 manifests, 0 added, 0 orphaned |
| node --test --test-name-pattern=WOO scripts/rust-ci-wiring.test.mjs | 0; two passed |

Cargo ran sequentially with the TypeScript compiler environment supplied. The
reconciliation writer also reported zero additions/orphans. The mandatory WOO
command appends socket_classes without removing a prior target. The integration
merge preserved the venue implementation; shared dependency changes were covered
by the fresh scoped check and native target above. No CI rerun was requested.

Native field values, reference identities, function kinds/arities, counts and
original error identities are asserted before source/wire comparisons. The
layout group constructs four instances, defines 44 instance fields and creates
50 callables. All five groups together construct eight instances. Independent
literal expectations cover the twelve public and eight private own-arrow sets,
five wrapper replacements, ticker values 31/37/41/43/47/53, report identities,
HMAC auth preimage/result, actual subscriptions and heartbeat setter replacement.
Enumeration ends at caller payload mutation. These finite fixtures do not prove
a complete JavaScript engine, arbitrary realm/altered-super semantics, or every
stateful child method path. Unexercised fixture primitives fail explicitly.

Earlier pushed measurements are separate: `01e535a2c18f993e7c412c23f7dc6759f9e20c1c`
failed check (101, missing fixture trait members/store argument),
`506b2d4b10cfb7e469060b75ce0253b9d88b46d3` failed check (101, fixture visibility),
and `1745ed70d64c4105b3877de60e83ebeeb2aa2005` passed check but failed native (101,
two JSON numeric-spelling fixture mismatches). Other gates were not run at those
failed SHAs. At `6344653b8d98118dab27389ff4638a67945983b6`, check/native/extractor/
wiring exited 0 with four groups, before the fifth boundary-cost witness and
class repricing. Those measurements are not reused for the gate SHA above.

Sub-agent review caught a fixture context copy, incorrect stats method name,
missing mapper original-argument identity, and immutable payload sidecar loss.
The first three were fixed; the last is an explicit maintained ingress choice
with its lost mutation and native/source discriminator recorded above. Follow-up
review found no further blocker and ran no gates. The exchange-class draft was
also reviewed separately; its two corrected metadata errors do not establish
exchange-class completion in this slice.

Six isolated controls each compiled and exited 101 at the named native assertion:

| Mutation | Named failure |
| --- | --- |
| topics field renamed | WOO-CLASS native field order |
| wrappers dispatch to handlePingEvent | WOO-CLASS native wrapper return |
| installed handlers acquire a different Store | WOO-CLASS native shared ticker fields |
| execution callback replaces the original report | WOO-CLASS native limit report identity |
| public ping uses logged send | WOO-CLASS native replaced send |
| private onOpen becomes a normal arrow | WOO-CLASS native async kind count |

The finite control enumeration ends at async kind. All six failed before a
source/wire comparison; no survivor or compiler-only failure was counted.

Exact bytes were restored; the same five-group target passed again (exit 0).
The scoped src/tests/scripts preservation diff against the gate SHA exited 0.
The full tracked diff against that SHA also exited 0 before adding this receipt.
This manual receipt is the only tracked descendant change; gates are attributed
to the gate SHA, not to the receipt commit.

Not run: the full WOO oracle target list, constructor target remeasurement,
workspace compilation, host/wasm clippy, full monorepo suites, live exchange
calls, CI reruns, other venue gates, full exchange-class assembly, or private
mutable-container timestamp proof.
