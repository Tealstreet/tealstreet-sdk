# Kucoin Spot public class binding evidence

Branch: `feat/kucoin-spot-public-bindings`.
Actual integration parent: `f853d56fd227c2106dba609141b1fdd44bcab125`.
Reviewed, unmerged feature ancestry ends at callback repair
`dbacbeaa502d976fd2d60cf51a04b3bff5166c43` (PR7922). That ancestor
includes the reviewed futures/Spot private class and account-response repairs
identified in their receipts. These are feature ancestors, not integration.

`spot_public_class::define_spot_public_class` supplies the real implicit derived
constructor and fourteen ordered fields, including eleven actual native arrow
bodies. It shares `PrivateClassOps` language capabilities with private classes;
`frame` is a pure view of the same live receiver and module bindings. Async
construction runs eagerly and retains completion through the language provider.
Connection and public-stream bodies invoke existing Rust algorithms. Native
routing/subscription code preserves source Get/Call order, lexical receiver,
UTF-16 templates, first-element elision versus two-value destructuring, and
split/index operations. No whole Kucoin member factory or root-string body is
provided by the host. The older `class_construction` roots remain a distinct
layout component. Constituent helper boundaries keep their existing declarations
and canonical references, including
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary` and
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`.

Three native groups finish before any wire decoding/source assertions. The
floor stops at streams/final cleanup: four native instances across layout,
eager connection/listeners/subscriptions/ping, and market-stream composition.
Assertions inspect the complete clean field set, all14 Define requests, all11
fresh callable metadata tuples, null defaults and native interval, eager token
request and current field updates, three original listener handles, native
subscription/heartbeat Number/String/Boolean fields and key order, live send
replacement after the pingAt setter, and a received-data getter before disposal.
The streams group inspects two original parser handles, two decoded candles,
one book with two levels on each side, original ticker identity and all eight
native update fields, retained second-parser identity after partial cleanup,
final deletion and five native subscription/unsubscription frames. A callback
replaces the next forEach entry; the class must call that current entry with the
exact original parsed object. Counts and distinct numeric expectations are
literal independent inputs, not lengths copied from the tested output.

The source oracle transpiles the maintained whole Spot public module unchanged
with ESNext class fields. It executes maintained endpoint/interval tables,
jsonParse and order-book helpers; a transparent parse observer retains actual
helper results for alias assertions. Base/socket/clocks/parent/Store are explicit
dependencies. The Rust fixture supplies finite language operations and explicit
native order-book dependencies. This is not a concrete engine/transport proof.
Failures, rejections, disconnected/nullish branches, arbitrary descriptor or
proxy traps and every protocol branch are outside this three-group floor.

The first measured check at
`61e0c483e93a590675abd563289ce915aa1befbe` exited101 on unused shared-fixture timer fields/method. The new
target now checks construction scheduled no timers and uses the shared method
operation for received frames; no production warning suppression was added.
That compile rejection is not counted as a semantic mutation.
At `41b9bb6978eb2ad18b2d7814aa72d2933af5640e`, scoped all-targets check exited0.
The shared fixture primitive equality was then corrected for undefined/null/bool.

| Gate SHA | Measured gate | Exit/result |
| --- | --- | --- |
| `9d4bcf459dd987c2c8e3af2a780bc1452f7dcf7f` | exact spot_public_class target | 0;one test, three native groups then three source cases |
| `9d4bcf459dd987c2c8e3af2a780bc1452f7dcf7f` | exact spot_private_class target | 0;one test |
| `9d4bcf459dd987c2c8e3af2a780bc1452f7dcf7f` | exact futures_private_class target | 0;one test |
| `9d4bcf459dd987c2c8e3af2a780bc1452f7dcf7f` | exact public_streams target | 0;one test,61-case floor |
| `8989a04b013b80a462871b27078d37ba88232aff` | scoped Kucoin cargo check, all targets, jobs2 | 0;zero errors |
| `8989a04b013b80a462871b27078d37ba88232aff` | focused KuCoin CI wiring | 0;22 checks including exact new target/drop control |

All Cargo targets used jobs2, tests used locked/single test thread and the
installed TypeScript compiler. One Cargo ran at a time; sub-agent review ran
only while Cargo was idle. The wiring-only descendant preserves source/tests/
scripts from `9d4bcf459dd987c2c8e3af2a780bc1452f7dcf7f`; no earlier gate is relabelled as freshly run there.

At baseline `8989a04b013b80a462871b27078d37ba88232aff`, four isolated
production controls and one separate fixture control compiled and failed in the
exact new class target. Each restored target then passed at exit0:

| Control | Mutant exit | Named failure | Restored exit |
| --- | ---: | --- | ---: |
| Extra token Define in initializer | 101 | KUCOIN-PUBLIC fourteen definitions | 0 |
| Open listener replaced by onClose handle | 101 | KUCOIN-PUBLIC original listener handle | 0 |
| pingAt assignment moved after send | 101 | KUCOIN-PUBLIC late send after pingAt setter | 0 |
| Stream cleanup retains matching parser | 101 | KUCOIN-PUBLIC cleanup retains original second parser | 0 |
| Fixture forEach snapshots entries | 101 | stale second callback (explicit fixture panic) | 0 |

The fixture control tests the instrument and is not counted as a production
mutation. The control floor stops at iteration5/forEach snapshot. No survivor
or compiler-failure substitute was observed in these five iterations. Exact
source/test/script byte restoration was checked before manifest reconciliation.
Read-only sub-agent review found no concrete blocking issue and supports closing
the assembly finding under the stated language contracts/constituent limits.
The reviewer ran no Cargo, gates or controls.

One class row moves FINDING/divergent to ported; zero pending rows move.
Independent tomllib census over12 Kucoin manifests, stopping at
kucoinspot.ws-public.ts.toml:149 ported /42 divergent /0 pending. Five leading
class/formatter FINDING rows remain; embedded causes are not declared closed.
The metadata-commit extractor result is recorded separately after measurement.

Not run: full venue/workspace/monorepo suites, host/wasm clippy, wasm builds,
real transport, broad engine conformance, live account/order requests or CI reruns.

## Integration merge and metadata gate

Metadata commit `c2768c3c69` follows the measured code without changing its
source/tests/scripts/package/wiring bytes. Reconciliation used the official
manifest parser/serializer, preserving annotations; parity:extract writer output
reported updated0/added0/orphaned0 before that commit. No manifest was regenerated.

Integration advanced during the slice. Merge head
`6f621bd95a5c0c1b358782a6d19ea83c63302069` includes actual integration
parent `d74a1c01f77439f9df61fa330063be194aab7576` and was pushed before gates.
At that exact merged SHA, parity:extract:check exited0 with output
would-update0/added0/orphaned0, and the fresh focused KuCoin wiring target
exited0 with22 checks. The integration change adds a types CI-test import to
the wiring file; no Kucoin target is removed or changed.

Scoped git diff --exit-code against code gate `8989a04b01` exited0 for Kucoin
source/tests/scripts and package.json. A second diff exited0 over all SDK Rust
source/Cargo manifests/lock plus maintained Kucoin TypeScript. Therefore no new
Cargo or venue-test execution is claimed at the merge SHA: the named older
code gates remain attached to their original SHAs, with byte preservation.
The added CI dependency import's own tests are outside the KuCoin name-filter;
no claim of running those imported tests is made.
