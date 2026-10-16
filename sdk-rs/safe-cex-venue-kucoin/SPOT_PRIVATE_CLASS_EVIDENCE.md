# Kucoin Spot private class binding evidence

Candidate branch: `feat/kucoin-spot-private-bindings`.
Actual integration parent: `f853d56fd227c2106dba609141b1fdd44bcab125`.
Reviewed, unmerged response-repair ancestor:
`708f5ed91ed9f5a12f58bf2d8f80750994bba13a` (PR 7889).
Merge `adff7c16b12d9066567570d405601943f2cdf9fe` retained integration's
Kucoin target-removal checks and the ancestor's mandatory library target.
No unreviewed feature ancestor is asserted to be integration work.

`spot_private_class::define_spot_private_class` installs the eight real Rust
arrow bodies and prototype `dispose`. It calls the existing native connection,
message-routing and order-lifecycle functions; the host supplies language
operations rather than whole Kucoin member factories or method-root labels.
`frame` and `orders` are pure views of the same receiver. The old
`class_construction` targets remain a construction/layout component, with the
limitations already recorded in `CLASS_CONSTRUCTION_EVIDENCE.md`; their old
provider factories are not the new canonical bound route.

The focused target runs four independently specified native groups before its
first wire or maintained-source comparison. It pins 13 ordered fields, eight
fresh arrows and the shared prototype method's name/arity/kind; eager token
request, retained connection completion, token/interval/socket state and three
actual listener identities; original native stringify objects and their
Number/String/Boolean fields and key order; live send replacement during the
pingAt setter and lexical arrow receiver; input destructuring before the guard;
pong state; the real order handler's missing-id path and separate balance timer;
timer coalescing, eager fetch, clearing before await, retained balance completion
and post-await disposal; balance identity and loaded-field spread; repeated live
timer reads, dynamic method receiver, prototype reparenting, ignored superclass
return and original clear-error identity. The enumeration floor stops at the
disposal clear-error case, not every possible engine value or method branch.

The source oracle transpiles the maintained whole Spot-private module unchanged
with ESNext class-field semantics. It loads the maintained tables, enums and
jsonParse. Base socket, transport, clock, timers and exchange/store dependencies
are explicit fixtures. The native finite value fixture implements those same
language capabilities; this proves the class composition against their stated
contracts, not a production JavaScript engine, a real socket transport, arbitrary
proxy/descriptor traps or exhaustive superclass/newTarget behavior. Existing
constituent-row divergences are not erased by the assembly claim. See
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`,
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary` and
`sdk-rs/parity/MANIFEST-SPEC.md#closed-typed-output-boundary` where those rows cite
them; this receipt does not redeclare those contracts.

Read-only sub-agent review found three initial proof gaps: wire-only outgoing
payload checks, no live prototype-parent witness, and missing prototype method
metadata. All were repaired before the candidate gate below. A fourth fixture
issue, integral f64 spellings, was repaired with the shared JavaScript JSON
encoder and number formatter after native kind assertions. All native groups
now finish before any wire assertion. The review ran no Cargo.

At `862a21de7844ac02678435ce2647d4b2af86d1ab`, scoped check exited101 on an
unresolved generic routing argument. The explicit `Option<&Rc<V>>` annotation
repairs compilation; that failure was not a discriminating control.

At pushed source SHA `0f453d359957fef0c45367ba8d539d4f1eaecf06`:

| Measured gate | Exit | Scope |
| --- | ---: | --- |
| cargo check -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 | Scoped crate, not workspace compilation |
| cargo test --locked --jobs 2 --test spot_private_class -- --test-threads=1 | 0 | One test, four native groups followed by four maintained-source cases |
| node --test --test-name-pattern=KuCoin scripts/rust-ci-wiring.test.mjs | 0 | 20 focused checks, including the new target and removal control |
| yarn parity:extract:check via the direct Yarn JS entrypoint | 0 | Output inspected: would update0, added0, orphaned0 |

The initial test process waited in `_dyld_start` before test execution; a process
sample established that boundary. Once started, the test reported pass in2.07s.
Raw generated logs and samples remain untracked outside the repository.

Workspace builds, the complete Kucoin/WOO suites, clippy, wasm checks, real HTTP
or websocket execution and CI reruns were not run for this slice. The focused
account/parser targets will be reported separately if rerun at the descendant
head; the ancestor receipt is not silently reused as a fresh measurement.

Five isolated compiling production mutations at source SHA
`0f453d359957fef0c45367ba8d539d4f1eaecf06` each failed the named native assertion
at exit101; each exact restoration reran the focused native/source target at
exit0. No survivor or compiler rejection was discarded from this five-control
floor; the enumeration stops at prototype metadata.

| Mutation | Named assertion | Broken / restored |
| --- | --- | --- |
| Initial ping interval5000→5001 | KUCOIN-BOUND initial interval | 101 / 0 |
| Capture sendWithLog before the pingAt setter | KUCOIN-BOUND live send after pingAt setter and lexical receiver | 101 / 0 |
| Skip post-await balance publication | KUCOIN-BOUND retained balance completion | 101 / 0 |
| Pass the class instead of the actual method receiver to super | KUCOIN-BOUND actual method receiver | 101 / 0 |
| Request dispose arity1 instead of0 | KUCOIN-BOUND prototype callable metadata | 101 / 0 |

After all controls, a scoped `git diff --exit-code` against that source SHA for
this crate's src/tests/scripts exited0. The class row then changed from
FINDING/divergent to ported; its earlier layout component and the constituent
method boundaries remain separately identified. This closes one owning-class
assembly finding, not all Kucoin findings or all acknowledged divergences.

At pushed descendant `8e590029cc9d3180fa39a101e92cc618805c8ce0`:

| Measured gate | Exit | Scope/result |
| --- | ---: | --- |
| cargo test --locked --jobs 2 --lib --test accounts --test spot_validation --test spot_private_class -- --test-threads=1 | 0 | Library1, accounts8, private-class1, validation10: 20 passed |
| yarn parity:extract:check via direct Yarn JS entrypoint | 0 | Output inspected: would update0, added0, orphaned0 |
| git diff --exit-code 0f453d359957fef0c45367ba8d539d4f1eaecf06 HEAD -- crate src/tests/scripts | 0 | Exact source/test/oracle preservation |

The scoped check and20 wiring checks measured at `0f453d3599` were not rerun
at this metadata descendant; their inputs are unchanged. The fresh focused
account/parser tests above cover the reviewed response ancestor on the merged
integration parent instead of relabeling its old receipt.

Independent tomllib census of all12 Kucoin manifests, stopping at
`kucoinspot.ws-public.ts.toml`: 146 ported /45 divergent /0 pending →
147 ported /44 divergent /0 pending,191 rows. Status moves: zero pending→ported,
zero pending→divergent, one divergent→ported. Seven class/formatter rows still
open with FINDING; embedded causes on other divergent rows remain separate work.
This is not a claim that the stale ten-finding assignment is fully closed.

Post-PR adversarial sub-agent review found an additional counting gap: slicing
13 keys from a fixture instance could ignore an appended field. Commit
`d79069a35bd02f80472872b936841e452dbbee0e` asserts the clean second instance's
complete own-field suffix and records every field-definition request, comparing
the entire per-receiver sequence against the independent13-name expectation.
The request sequence also catches duplicate definitions that an object-key
projection would collapse. This was a static review finding; no old-test mutation
survivor is claimed to have been measured.

At pushed `d79069a35bd02f80472872b936841e452dbbee0e`, scoped all-target check0,
focused private-class test0 (one test/four native groups then four source cases),
Kucoin wiring20/20 at exit0, and extractor check0 with zero updates/adds/orphans.
Two further isolated production mutations compiled and failed by name:

| Mutation | Named native assertion | Broken / restored |
| --- | --- | --- |
| Append extraBoundField | KUCOIN-BOUND complete clean-instance field set | 101 / 0 |
| Define token a second time | KUCOIN-BOUND complete thirteen field definitions | 101 / 0 |

Both restorations reran the native/source target; scoped source/test/script
preservation against `d79069a35b` then exited0. This two-control floor stops at
the duplicate definition. The earlier five controls remain attributed to their
actual source SHA. The19 account/parser/validation tests from `8e590029cc` were
not rerun after this fixture-only repair; production inputs are unchanged.
