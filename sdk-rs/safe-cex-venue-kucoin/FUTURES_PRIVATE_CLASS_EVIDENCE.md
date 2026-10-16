# Kucoin futures private class binding evidence

Branch: `feat/kucoin-futures-private-bindings`.
Actual integration parent: `f853d56fd227c2106dba609141b1fdd44bcab125`.
Reviewed, unmerged feature ancestors: Spot private class
`a7a852765991a85d356fd7c801dfbc91188bcf8b` (PR7913) and response repair
`708f5ed91ed9f5a12f58bf2d8f80750994bba13a` (PR7889).
They are identified as feature ancestors, not integration work. No unreviewed
feature ancestry is asserted to be covered by another lane's gates.

The canonical `futures_private_class::define_futures_private_class` installs
fourteen ordered fields and ten actual Rust arrow bodies. It calls the existing
native connection, routing, normal-order and algo-order algorithms. Reconnect
keeps its previous socket, evaluates optional method arguments only after the
nullish checks, clears ws before close, resets live fields and awaits the real
installed connect arrow. Dispose is an own lexical arrow, with class home kept
separate from receiver and resolved against the current prototype parent.
Subscription payload creation precedes send lookup, as in maintained source.
No host factory supplies a whole Kucoin member body or a root-string stand-in.

The new class shares `PrivateClassOps` language capabilities with the Spot
class. `frame` and `orders` remain pure views of the original live receiver;
async arrows execute eagerly and retain completion independently of caller
promise ownership. The finite test runtime was extracted into a shared support
module; the existing Spot case functions were preserved. This is not a concrete
JavaScript engine or a production websocket transport. The older
`class_construction` functions remain a separate layout/construction component.
Existing method/helper divergences keep their own declarations. Relevant
canonical references remain in their rows, including
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy` and
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`.

Three independently specified native groups finish before the first wire/source
assertion. The floor stops at reconnect/disposal clear-error, after layout,
ingress/order/timer behavior and reconnect/dispose. Five native class instances
are constructed across those groups. Assertions inspect the complete field set
and all14 definition requests; ten fresh callable metadata tuples; original
listener handles; eager request and retained completion; original native frame
objects and Number/String/Boolean fields; parent mapper receiver and exact raw
order references; fill price11 and contract amount21; original order/status
identities; live replacement of the second array element during the first
callback; welcome subscription guard; debounce and clear-before-snapshot;
optional-call receiver; outer reconnect pending until inner completion;
lexical disposal, live class home, original clear-error identity and disposed
refresh scheduling/callback suppression.

The source oracle transpiles the maintained whole futures private module
unchanged with ESNext class fields. It loads maintained tables, enums and
jsonParse. A transparent parse observer retains the actual helper's result for
reference assertions. Base, transport, clocks, timers, parent mapper and Store
are explicit dependency fixtures. It constructs the corresponding five source
instances. This finite matrix does not certify arbitrary proxy/descriptor traps,
all superclass/newTarget behavior, every protocol branch or real transport.

Read-only sub-agent review found two initial fixture gaps: count-only mapper
checks and snapshot forEach iteration. The mapper now asserts original parent
and raw-data identities in both runtimes. forEach captures length but reads each
current element; the Store callback replaces the second order to prove the
class reaches that live element. A further pending-before-completion assertion
was added for reconnect. No old-test mutation survivor is claimed from these
static findings. The reviewer ran no Cargo.

At pushed SHA `a106fdc5bb0a72e9cab44e613a4735cdc975b333`:

| Measured gate | Exit | Scope/result |
| --- | ---: | --- |
| cargo check -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 | Scoped crate/all targets, zero errors |
| cargo test --locked --jobs 2 --test spot_private_class --test futures_private_class -- --test-threads=1 | 0 | Two target tests; four Spot and three futures groups before their source comparisons |
| node --test --test-name-pattern=KuCoin scripts/rust-ci-wiring.test.mjs | 0 | 21 focused checks, including new target/removal control |
| yarn parity:extract:check via direct Yarn JS entrypoint | 0 | Output inspected: zero updates/adds/orphans |

At pushed SHA `6d189b599b079ed676dc4960ef60f7b861392e6b`, after adding the
reconnect pending assertion, scoped all-target check0 and both private-class
targets0 were measured afresh. This updated test result is not attributed to
the earlier candidate. Source production and wiring/extractor inputs are
unchanged from that earlier candidate; the metadata status change follows the
controls below and receives its own extractor measurement.

Four isolated production controls and one separately identified fixture control
at `6d189b599b` compiled, failed by name at101, and each exact restoration
reran the futures native/source target at0:

| Kind/control | Named native assertion | Broken / restored |
| --- | --- | --- |
| Production: duplicate token field definition | KUCOIN-FUTURE complete fourteen definitions | 101 / 0 |
| Production: bypass subscribed guard | KUCOIN-FUTURE welcome subscription guard | 101 / 0 |
| Production: replace raw algo argument with empty object | KUCOIN-FUTURE original raw order reference | 101 / 0 |
| Production: drop inner connect promise instead of awaiting | KUCOIN-FUTURE reconnect waits for inner completion | 101 / 0 |
| Fixture: snapshot forEach elements | KUCOIN-FUTURE original native order identity | 101 / 0 |

No survivor or compiler rejection was discarded in this five-control floor,
which stops at the forEach fixture control. Exact scoped src/tests/scripts
preservation against that SHA exited0 after restoration. Raw generated logs
remain outside the repository; this is a manually authored receipt.

Workspace/full-venue/full-monorepo suites, the remaining Kucoin test targets,
clippy, wasm, real HTTP/websockets and CI reruns were not run for this slice.
The19 account/parser/validation tests measured in the ancestor's receipt were
not rerun here. A census is not a proof that other FINDING causes are closed.

Independent tomllib census of all12 Kucoin manifests, stopping at
`kucoinspot.ws-public.ts.toml`: 147 ported /44 divergent /0 pending →
148 ported /43 divergent /0 pending,191 rows. This slice moves one
FINDING/divergent class row to ported, zero pending→ported and zero
pending→divergent. Six leading class/formatter FINDING rows remain; embedded
causes on other divergent rows are not silently counted as closed.

At pushed metadata descendant `4f07b9767f16c3b34e50906692194c6ac515e81e`,
extractor check exited0 with zero updates/adds/orphans. Scoped src/tests/scripts
and package/wiring preservation against `6d189b599b` exited0. Cargo and wiring
were not relabeled as rerun at that metadata descendant.

Post-PR read-only adversarial sub-agent review of PR7919 at that descendant
reported no concrete blocking finding and supported the class status move under
the stated contracts. It verified preservation and the148/43/0 census, and
inspected recorded controls without running Cargo or gates. Nullish optional-call
branches, connection failures and all protocol branches remain outside this
finite composition matrix; the existing constituent tests were not rerun here.
