# WOO retained public listeners

Two listener bodies now have native implementations. Parent
`6d6c705355e341586250858258972c454a105dd7` carries locally reviewed unmerged
PRs 7858, 7862, 7867, 7876 and 7880. Initial integration base was
`ac0f5fccf24207b26c279e67da2a88f2c72a7aae`. Before pushing, merge
`d6a01011bb5b8a2e70bf6503180ccb6e3c30b3da` brought in integration
`1a43f3a8f25b7e5fe1c238794e79bad11e4c4788`; ancestry is explicitly stacked.

Rust owns parsers, subscription/cleanup, snapshot async continuations, buffering,
retry and finally behavior. The opaque operation contracts retain property,
iterator, coercion and eager non-cancelling promise behavior. Imported helpers
retain their separate mappings and domain costs. Typed numeric test adapters
call native shared book update/total helpers; the calendar fixture includes the
actual existing native module. Book level identity through those typed fixture
adapters is not proved; outer book alias and later live contents are asserted.

Five native assertion groups cover eleven source cases, stopping at snapshot URL
interpolation. Native fields/types, counts, reference identity, timers, buffered
updates, source callback effects and error/retry behavior precede source calls.
The fixed fixtures are ordinary records, finite numeric inputs and ASCII symbols;
they do not establish exhaustive realm/descriptor/class assembly behavior.

Review corrected interpolation timing: topic symbol coercion precedes live
interval access, and endpoint coercion precedes reverse-symbol evaluation.
The fixtures include the source memoization cache returning an object and a
throwing endpoint conversion. Source quirks are preserved: candle connection
polling survives cleanup, and a pending book snapshot can repopulate retained
state after cleanup. The measured receipt follows.

The shared finite promise/value test fixture was extracted without changing
its operations; both its existing lifecycle target and the new target will be
run. Mandatory WOO CI includes the new target.

Canonical declarations remain in
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

## Measured receipt

Gate SHA `cf68b7aef60f0106dbc9fdf2ef1605d83e6e41e8`.
Independent tomllib: 220 ported / 308 divergent / 4 pending, 532 rows in
21 manifests, stopping at woo.ws-public.ts.toml. Two pending→ported against
parent6d6c705355, 34 cumulatively against the initial integration base.

Measured exits at that SHA:

- Scoped WOO all-targets Cargo check, jobs 2: 0.
- Focused legacy_lifecycle and public_streams targets, jobs 2 / one test thread:
  0, five passed in each target. Both ran because the fixture was extracted.
- Extractor check: 0, 7341 symbols / 512 files, zero updates/additions/orphans.
- WOO-only CI wiring tests: 0, two passed.

Earlier merged SHA d6a01011bb check exited101: missing CanonicalOps import in
an existing test after fixture extraction. Check at965b3de22d exited101 on
borrowed temporary expected arrays. Both were fixture compile failures; no native,
extractor or wiring gates ran at those SHAs. They are not sensitivity proofs.

Review corrected two interpolation timings and five observation gaps. Native
request URLs/counts, exact loaded booleans, registered topic values, cleanup-stat
arguments/order and distinct stale bid997 now precede source capture. Using a
stale price different from the later valid price prevents replay from hiding an
incorrect timestamp filter. The five prior lifecycle test bodies and extracted
fixture operations remain unchanged apart from module visibility/imports.

Eleven isolated compiling controls exited101 in native test execution before
source capture: candle field source, public-only guard, stale filter, retry
loading reset, topic interpolation, snapshot interpolation, cleanup boolean,
wrong topic dictionary, cleanup-stat callee, wrong topic key and snapshot URL.
Each edit was restored before the next. The wrong-dictionary edit failed at the
fixture setter boundary; it does not attribute sensitivity to the topic-list
assertion. A separate wrong-key edit failed that native topic assertion. The
topic interpolation edit failed the decoded handler-array kind check. The
remaining controls failed independent named native value/count/type assertions.
Both control runners ended with byte-exact restoration and all five new target
groups passed0. The existing target was not rerun after controls because no
shared fixture or lifecycle input changed. No tested edit survived this finite
set; no untried mutation or exhaustive runtime coverage is implied.

Local sub-agent review independently checked census, mapped targets, preserved
exemptions, fixture extraction, portability, raw gates and native-before-source
placement. Reviewer ran no Cargo/gates.

Not run: workspace compilation, full venue suite, public_canonical/shadow_compare
reruns, host/wasm clippy, wasm compilation, live exchange tests, exhaustive
realm/descriptors/class assembly or unrestricted imported-helper parity.
