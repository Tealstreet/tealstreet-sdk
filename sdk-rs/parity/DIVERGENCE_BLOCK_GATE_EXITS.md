# Five-venue divergence audit measurements

The audit parent is integration
`f853d56fd227c2106dba609141b1fdd44bcab125`. The named five-venue census was measured
with Python 3.14 tomllib from that immutable Git tree, with independent expected
divergent totals 54/25/16/13/40: exit **0**, total **148**, pending **0**, open
findings **0**. Frozen row values and generated captures stay outside the repo.

## Requested workspace host clippy

The verification branch combines reviewed audit head
`cb3a126730b5622e9ae83c5e7f9d3bd761b07cce` with Aster repair head
`07c3d6ee391d14606bdcd8822d49c327bea9a460`, producing measured merge SHA
`669e0246b5d3a4eb8fc877901b8d54c1e429e8ba`.
Both PR 7912 and PR 7885 were open at measurement; this verification branch has
both as ancestors and is separate from the fresh five-venue audit branch.

From sdk-rs, with SAFE_CEX_TYPESCRIPT_PATH set:

`CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets --locked`

Exit **0**, finished in **2m08s**. The tracked tree was clean throughout the run;
no production mutation or test input changed during compilation. This measures
the combined audit and Aster repair, including Bybit's standard no-op waker fix.
The exact merge commit is pushed on chore/rust-divergence-host-verification.
No wasm, workspace build, runtime test suite or CI rerun accompanied this lint
measurement. The five-venue audit does not inherit those open feature branches.

## Source observations and discriminating controls

The Phemex source supplement was committed at
`28590355e1f128c4730a358d7eab00d4ed67d524`; its source-only Node invocation
returned **0**. A separate assertion for validation emission was then committed
at `822dd79647441b13df4788be28f54470127c57a5`.
At that SHA, with SAFE_CEX_TYPESCRIPT_PATH supplied:

`node sdk-rs/safe-cex-venue-phemex/tests/divergence_audit.cjs`

Baseline **0**, nine independently fixed observation identities. Source values
and effects are asserted before JSON output. Seven unit-bearing method/record
observations, the separate public validation return, and immediate reconciliation
effects are source observations; no native Rust execution is implied.

| In-memory source control | Exit | Required assertion | Restored exit |
| --- | ---: | --- | ---: |
| `SAFE_CEX_BLOCK_SOURCE_CONTROL=symbol-in` | 1 | `PHEMEX-BLOCK returned uppercase unit` | 0 |
| `SAFE_CEX_BLOCK_SOURCE_CONTROL=sync-clear` | 1 | `PHEMEX-BLOCK source cleared symbols before return` | 0 |

Each control changes only the selected source initializer held in memory; the
maintained TypeScript file is unchanged. Both failed by name and both restored
passes completed. These two controls had no survivors. No new native production
mutation or native survivor claim is made for this block.

All source supplements below used Node **24.20.0/arm64**. Source schemas are
compiler-derived declared-shape observations, not TypeScript runtime validators.
The read-only reviewers executed and reported these bounded observations:

| Scope | Measured SHA | Exit | Observation |
| --- | --- | ---: | --- |
| Bitunix source method, request-carrier, subscription, existing wire and existing REST-read oracles | 822dd79647441b13df4788be28f54470127c57a5 | 0 for each of five commands | 24 method records including 17 eager prefixes; three ordinary-JSON inherited stop-type results; exact returned/key units; API header; two valid-interval subscription channels; actual response schema/raw maximum values. |
| Bitunix real AxiosHeaders follow-up | 822dd79647441b13df4788be28f54470127c57a5 | 0 | Source-shaped plain request object retains api-key D800 in actual AxiosHeaders. This is configured-adapter/header observation, not network admission. |
| Bitrue/Orderly combined source probe | 050ced4f7f4cf4b1f103d4d726e30847cded6d95 | 0 | Eight Bitrue and five Orderly response boundaries, six Bitrue immediate prefixes, exact key-sensitive quantity/market/margin results, symbol/position-ID observations. |
| Bitrue private symbol supplement | 822dd79647441b13df4788be28f54470127c57a5 | 0 | Actual reverseWsSymbol on D800_USDT returns E-D800-USDT, satisfying the real underscore guard; downstream private order-symbol use inspected. |
| Orderly public base64url supplement | 822dd79647441b13df4788be28f54470127c57a5 | 0 | Actual exported function returns [D800,45,95] from a valid unrestricted string argument; no dependency substitution. |
| Breakout Prop records and timeout/public-validation chain | 050ced4f7f4cf4b1f103d4d726e30847cded6d95 | 0 | Seven complete response records retain D800; timeout error returns publicly at length34/index33, with report/finally effects observed. |
| Breakout Prop five cause probes through existing whole-source fixture loaders | 822dd79647441b13df4788be28f54470127c57a5 | 0 | Constructed options key, real handler-key lookup/callbacks, login credentials/token, valid-enum placement instrument and configured API URL retain D800. Eight existing MIXED causes gain support. |

The Bitunix reviewer also disclosed three harness development failures: an
initial temporal-dead-zone ReferenceError whose numeric exit was not retained;
a missing dependency/eager-effect assertion **1**; and a real-Axios harness shape
assertion **1**. Corrected source probes returned **0** at the named SHA above.
The Bitrue/Orderly probe initially returned **1** for a missing-fixture-brace
SyntaxError before source execution, then **0** after correction. These are
harness failures, not deliberately broken production controls. Failed development
probes without a retained SHA/exit are not attributed to a tested commit.

Temporary captures and probe-development files remain outside the repository.
The permanent Phemex supplement and existing venue oracles remain executable;
the per-row audit tables name the traced observations and native limitations.

## Reconciliation, classification and census

At `2a0310ba22d34e69c3ea3d6fe5e3e29e63e46c02`:

| Command | Exit | Measured result |
| --- | ---: | --- |
| `yarn parity:extract:check` through explicit Node/Yarn | 0 | Output read: 7,341 symbols, 512 files, would update **0**, added **0**, orphaned **0**. |
| Python 3.14 tomllib ordered-row comparison against integration parent | 0 | Triples below; **zero status/target changes**; exactly **89 new finding notes**; every historical annotation retained. |
| `git diff --exit-code` against workspace-host SHA 669e0246 over the five Rust src and five TypeScript venue trees | 0 | Those ten production source trees are byte-identical to the measured workspace host input. This is input equality, not a new compilation. |

| Venue | Ported/divergent/pending | New finding notes | Last frozen iteration |
| --- | --- | ---: | ---: |
| bitunix | 68/54/0 | 39 | 54 |
| bitrue | 82/25/0 | 22 | 25 |
| orderly | 79/16/0 | 11 | 16 |
| phemex | 66/13/0 | 9 | 13 |
| breakoutprop | 14/40/0 | 8 | 40 |

The canonical writer ran after the manifest edits and returned **0**, updated
**0**, added **0**, orphaned **0**. Existing rows were reconciled; no fixture,
annotation set or rust_files array was regenerated. The independently frozen
floors are 54/25/16/13/40; all 148 source/symbol/iteration identities match them.
Authored classifications reconcile as **58 supported / 89 findings / 1 unresolved**;
kinds are **65 explanation / 21 scheduling / 3 behavior**. The 147 original
CHOSEN-containing rows are **58/88/1**; Phemex's CORRECTED-only row is separate.

Not run for this audit: new Rust runtime/decoder/scheduling tests or native
mutation controls, wasm compilation/clippy/runtime, workspace build, full test
suites, Node 26/other architectures, live credentialed HTTP/WebSocket requests,
or CI reruns. Native counterparts were code-inspected. The requested full host
workspace clippy result remains attached to the combined repair/audit SHA
669e0246 above; the fresh feature branch does not inherit the open repair PR.

## Cross-review corrections and public Bitunix consumer gate

At `312d0f629c318f1060a684b5bf3de07aca445fd1`, adversarial read-only cross-review
passed Bitrue 25, Orderly 16, Phemex 13 and Breakout Prop 40. Bitunix review
challenged five witnesses. Iterations 2/4/41 were withdrawn to unresolved with
original notes restored; their private-helper observations lack a proved outgoing
consumer. Iterations 8/48 gained the actual public Position return chain.
The earlier 89-finding census above remains a historical measurement, not the
post-review result.

The durable Bitunix probe's first five executions at
`6845d32d35dcff2cba373483c2448c38a2e26cb3` each exited **1** while loading an
unused get-key dependency unsupported by the existing oracle loader. Neither
control reached its source mutation; those failures are harness failures and
provide no discriminating proof. The unused dependency was removed in the next
commit. The temporary public-consumer probe at 312d0f6 had already exited **0**.

At `eee9542bef3f6a938685c37f7b37c4d8ad6b4c94`, Node 24.20.0/arm64, with
SAFE_CEX_TYPESCRIPT_PATH supplied:

`node sdk-rs/safe-cex-venue-bitunix/tests/divergence_audit.cjs`

Baseline **0**. Actual source fetchPositions calls the actual candidate traversal,
row selection and real symbol field-list lookup; one returned Position has
symbol/id unit D800 and contracts 2. Assertions precede serialization. Independent
trace expectations are one GET, one candidate traversal, one selection and two
symbol-field reads. This is a source-only gate, with no new native execution or
CI discovery claim.

| In-memory source control | Exit | Source-caught assertion | Final failed assertion | Restored exit |
| --- | ---: | --- | --- | ---: |
| `SAFE_CEX_BLOCK_SOURCE_CONTROL=candidate-data` | 1 | `BITUNIX public candidate row` | `BITUNIX public position count` | 0 |
| `SAFE_CEX_BLOCK_SOURCE_CONTROL=first-value` | 1 | `BITUNIX actual first-value unit` (65533 versus 55296) | `BITUNIX public position count` | 0 |

The source method catches the first assertion and returns its fallback; the final
public-result count fails independently. No swallowed assertion is claimed to
propagate. Neither control survived. Maintained TypeScript remained unchanged.

At that same SHA, extractor check exited **0**, output read: 7,341 symbols across
512 files, would update **0**, added **0**, orphaned **0**. Recursive tomllib
comparison against f853d56f exited **0**: triples remain 68/54/0, 82/25/0,
79/16/0, 66/13/0 and 14/40/0; new finding notes are **36/22/11/9/8**. Status,
target, row identity and every historical annotation are unchanged. An initial
shallow census invocation exited **1** because it omitted Orderly's six nested
ported wrapper rows; the recursive invocation includes them and passes.
The writer after the correction returned **0**, updated/added/orphaned **0**.

Corrected classifications are **58 supported / 86 findings / 4 unresolved**,
with **62 explanation / 21 scheduling / 3 behavior** findings. The original
147 CHOSEN-containing rows give **58/85/4**; Phemex's CORRECTED-only row remains
the additional scheduling finding. Final frozen iterations remain 54/25/16/13/40.
Read-only Bitunix re-review passed these five corrections at eee9542b; no further
native gates were run. The not-run list above still applies.
