# Diagnostics and merge resolution

Original preparation parent:539ac8165b8e36ef009dc467123e69fe14e002e2.
Resumed integration parent:dcb88b8e64efc8da8ed831388c9adaec2609b77f.
Delivery branch:fix/rust-shared-diagnostics. The previously reviewed local
preparation branch is included through merge e92069cd4e8861a0a720f66e3544ce0177b20ed6;
no unreviewed feature work is added.
This finite unit resolves nine original explanation findings: five require code
and four require owner rationale. Status/target movements:0. The five rows expose
three effect mechanisms: sealed existing-field writes, repeated timestamp reads,
and eager selection before deduplication. These new counterexamples do not
retroactively turn the historical explanation audit into behavior proof.

| Family | Selected symbols | Resolution |
| --- | --- | --- |
| Sealed stamps | markRestRequestStart; markRestProxyHeader; stampSigned; markRestSigned |4 code plus residual rationale |
| Observable merge | mergeUpdatesUsingLastUpdate |1 code plus residual rationale |
| Config/header/auth | readConfigNumber; proxyHeaderOf; isAuthOperationObject |3 rationale |
| Numeric aggregate | percentile |1 rationale |

Declarations live once at
[diagnostic adapters](MANIFEST-SPEC.md#diagnostic-adapter-choice),
[percentile](MANIFEST-SPEC.md#diagnostic-percentile-choice) and
[merge readers](MANIFEST-SPEC.md#merge-reader-choice); row notes cite them.
Existing declarations remain canonical at
[14](MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](MANIFEST-SPEC.md#numeric-ingress-defect),
[18](MANIFEST-SPEC.md#divergence-taxonomy), and
[19](MANIFEST-SPEC.md#forced-surrogate-conflict).

Six native tests first failed101 at148639c9d5ec525cfe0f4744ff77e15bc8cefac1;
all six passed0 at781393c60fc542e1a49001e628f920e64c1e4eaf. Tests assert
native fields, errors, Rc/pointer identities, independent read counts and the
fourteen-effect trace. They do not serialize the tested values. Additional
zero/-0/NaN cases establish presence-based writes, not sign-bit preservation.
The maintained-source fixture then checks twenty cases with unchanged function
bodies, strict-mode transpilation, actual lodash and an injected clock. Its
native JavaScript assertions precede the completion marker. A parent guard
rejects an empty successful producer. No fixture snapshots were regenerated.
Exact measurements and controls are in DIAGNOSTICS_CHOICE_GATE_EXITS.md.

A bounded rg read of packages/safe-cex/src, ending after six production merge
calls, finds ApexOmni, Lighter, Bybit and Bybit Inverse as source consumers.
Shared stamping consumers are safeCexAxiosCreate (two request-start calls),
remoteSigningAdapter and transport/tealstreet/tealstreetTransport. These are
source call sites, not proof that every corresponding native production path
invokes this repaired utility. No all-25-venue inheritance claim is made.
Tests inject admitted configs/readers and do not prove native HTTP assembly.

Two read-only sub-agent reviews passed production sequencing and test/source
instrumentation. Review identified direct helper return-status and dynamic
property costs that the canonical declarations now price explicitly. Historical
audit counts and unrelated remaining findings are unchanged.

## Resumed delivery

The integration stack is now merged. This slice retains exactly nine note-only
manifest changes against dcb88b8e64, with zero status/target/layer/row changes.
The recursive utils census after this slice is160ported/55divergent/0pending,
with20other findings retained. GRVT is113ported/51divergent/0pending and0findings.
Store, transport and CrossTrade also have0pending in the measured directory
scope. This is a census, not proof of complete function conformance.

At e92069cd4e8861a0a720f66e3544ce0177b20ed6, six regression and four existing
native tests pass before source comparison. One ordinary REST test then passes
(two filtered), followed by the twenty-case actual-source gate. Both deliberate
source controls again fail1 at the named assertion and restore0. No fixture is
regenerated. The original six native pre-repair failures stay attached to their
historical commit; these new successes do not transfer an old gate exit.
