# Accepted-set gate evidence

## Native source-tooling contract additions

On source base `04171f4f19`, all production targets in `safe-cex-parity`
produced 32 native/default receipts: 20 runtime results and 12 classifications.
The new `fragment_named`, `local_types_with_recipes` and
`parity_trait_inventory::collect` contracts execute actual native operations
against independently authored token/file/AST witnesses. The concrete
`ModuleRecipes` alias executes ordinary compiler identity and decoder/conversion
trait probes; its exact native-tooling declaration claim does not skip them.

All four new contracts pass. The package's five existing positional-array defect
owners match the scoped standing ledger exactly, with no new or stale scoped
shape failures. A plain package run reports those five known defects; the scoped
ratchet comparison passes. This is native tooling evidence, not a full workspace
or feature/wasm matrix result. The ledger and production Rust sources are unchanged.

## Initial native census

Native receipt investigations can select a finite cohort with repeatable
`--module=signing` or `--owner=signing::serialize_query` selectors alongside
`--package=safe-cex-venue-binance --measure`. Selectors form a union, include
module descendants, and must each match compiler owners. Exact canonical owner
names work too. Inventory discovery and source checks remain complete; only
native measurement receipts are scoped. All local helpers in selected source files
are retained conservatively, including helpers in separate Deserialize impls;
delegated helper obligations follow their selected native runtime owner.
Filters are rejected by the enforcing gate and recorded in receipt configuration.

Base: `7fcffd57e8`, branch `feat/rust-accepted-set-gate`. All 26 venue crates
are scaffolds in this snapshot; unmerged venue repairs are absent.

The compiler inventory found 132 actual `Deserialize` owners, including private
and tooling types. Production runtime probes produced 131 results: 102 owners
accepted additional shapes (36 tagged-object enums, 66 positional-array
records), 24 required explicit positive fixtures, and five passed the current
shape probes. Generic `frames::HttpRequestArgs` failed a named assertion requiring
an explicit instantiation. It was not counted as a shape finding. These are
observed accepted shapes, pending classification against their TS/tooling
contracts, not 132 defects or a claim of full accepted-set equivalence.

| Crate | Owners accepting additional shapes |
| --- | ---: |
| safe-cex-types | 78 |
| safe-cex-transport | 14 |
| safe-cex-crypto | 4 |
| safe-cex-parity | 4 |
| safe-cex-store | 1 |
| safe-cex-candle-cache | 1 |

Invocation: `node sdk-rs/safe-cex-parity/scripts/accepted-set.mjs --census`, with
rustup binaries on PATH. The command failed at the aggregate assertion, after
all scoped crate probes ran. Cargo jobs were capped at two; no workspace-wide
suite was invoked. JSON results and individual test logs are written under
`sdk-rs/target/accepted-set-*`. Positive generated record samples were first
accepted by the production JSON decoder before testing their full arrays.

The first implementation has no passing enforcing mode. Inventory and census
are diagnostic scaffolding; classification, configuration coverage and runtime
mutation controls must close before the final gate is deliverable.

## Classified production-native measurement

The initial measurement above is immutable. On the same production base
`7fcffd57e8`, explicit fixtures and generic instantiation resolve the previously
unmeasured owners. Production-native probes now measure **115 outstanding shape
defects among 132 Deserialize owners**, plus 12 separately classified
constructor/conversion-only owners and two function-local non-decoder types.
All 26 venue crates remain scaffolds in this branch snapshot.

| Crate | Deserialize owners | Outstanding shape defects |
| --- | ---: | ---: |
| safe-cex-types | 104 | 89 |
| safe-cex-transport | 15 | 15 |
| safe-cex-crypto | 5 | 4 |
| safe-cex-parity | 6 | 5 |
| safe-cex-store | 1 | 1 |
| safe-cex-candle-cache | 1 | 1 |

The count changed through improved measurement, not production changes. Review
found explicit map samples with wrong positional ordering and unrelated extra
keys; those could falsely pass the sequence rejection check. The runner now
reads Serde's actual field order before constructing arrays and rejects missing
fixture fields. Explicitly uninhabited capability fields cannot supply a JSON
positional value and are documented in their contracts. Fixtures are accepted
by the real JSON decoder before any rejection claim. Positive-fixture failures
cannot enter the outstanding-defect ratchet.

Native probes now run the production library through a temporary executable,
without `cfg(test)`. An injected production/test decoder split is rejected by
name. Each crate's copied sources and manifest are restored before the next
crate runs. No production Rust source is repaired or modified by this gate.

The sixteen primary mutation controls cover enum/record widening, an unknown
public owner, remote-Self API classification and its concrete qualified bypass,
FromStr alias classification, local manual/borrowed/union decoders, a
conversion-only owner, a const-local decoder, a production/test decoder split,
a local FromStr-only decoder, local and namespace-shadowed macro expansions,
and private-construction Clone. Two further mutations prove that a classified
FromStr decoder rejects a case alias and that an inert adapter fails its
positive witness. Every control requires an assertion and rejects compiler
failures; strict native and wasm fixture baselines pass. The first
remote-Self control attempt exposed an unused generated import, which the
harness rejected as invalid evidence before that instrumentation was fixed.

## Prediction before shared repair integration

Before importing `feat/rust-p4-utils` repair `3bbc52657f`, the expected delta is
89 current shared-type owners (including `StoreUpdateIds`): 89 removed debt owners,
leaving 26 on this snapshot if that repair covers the full measured public
surface. Its new private codec companions independently require classifications
for their internal representations; those are not additional outstanding defects
and must not obscure the public-owner delta. A different measured delta is a
finding to investigate, not a reason to rewrite this prediction.

## Enforcing configuration matrix

The standing matrix inventories every workspace package and valid feature
combination for native and the checked-in wasm package policy. It executes
production decoders without test configuration. The measured snapshot has 286
receipts: 146 native owners and 140 wasm owners (the parity tooling crate is
native-only), resolving to the same 115 defect owners. The 146 includes 132
Deserialize owners, 12 alternate-only owners and two local non-decoder types.
Feature combinations with no owners remain inventoried rather than skipped.

Outstanding failures are fingerprinted by owner, target, enabled features and
exact forbidden input/path. Thus an existing native failure cannot conceal a
new wasm acceptance. The initial debt contains only measured shape acceptance,
never missing classifications, unsupported discovery or rejected positive
fixtures. Full Git ancestry enforces removal-only history, including merges.
The gate fails on a surviving stale entry as soon as its production probe passes.

Discovery rejects unknown procedural expansion, shadowed known macro origins,
and unsupported local owner forms at named assertions. Local Deserialize or raw
conversion owners must move to module scope for concrete API classification;
they cannot acquire a local exemption. Expected string spellings and union
corpora are reviewed contracts, not a proof of all possible input semantics.

## Delivery validation

Implementation commit: `266f305ac1` on `feat/rust-accepted-set-gate`.
The enforcing matrix passed end to end after the final discovery repairs:
146 distinct production owners, 115 outstanding defect owners, no new or stale
failure fingerprints. After committing the initial debt, its full-history check
and comparison against those saved matrix results also passed.

Scoped validation passed: 69 parity Rust tests, plain strict all-target Clippy,
crate rustfmt and runtime-helper rustfmt, 20 accepted-set Node tests (including
18 assertion-failing mutation cases), and 104 CI-wiring audit tests. The existing
extractor reported zero manifest drift, and parity check/report passed. The
extractor initially hit a missing local smol-toml installation; an isolated
lockfile-based dependency install inside this worktree resolved it, and the full
extractor then passed. No production repair, full-workspace build, or monorepo
suite was run. Sub-agent review returned MERGE after the reported discovery and
adapter gaps were closed. Hosted CI has not been claimed as executed here.

## Adversarial review corrections before repair integration

The earlier prediction of 90 removed and 25 remaining double-counted a supposed
StoreUpdate owner that does not exist in this debt. StoreUpdateIds is already
one of the 89 shared-type owners. The store debt is SocketDiagnosticsSource,
which the cited shared repair does not change. Corrected pre-merge prediction:
**89 removed, 26 remaining**, with new private codec classifications separate.

The 1,136 initial failure fingerprints all have empty feature lists. Features
are enumerated and fingerprinted by the mechanism, but the feature-bearing
exchange/FFI crates currently have no debt owners. Independent review injected
a feature-only regression and observed its rejection; the initial measured
115-owner corpus itself does not establish agreement across nonempty features.

## Adversarial discovery repair

The review reproduced a production `cfg(not(doc))` widening that the original
rustdoc inventory missed. Discovery now checks an independent source module graph
before rustdoc, including inner attributes in external and path-attributed modules.
Raw `r#doc` predicates and raw cfg attribute names fail closed; block-local external
modules require module-scope coverage. Nine new named controls cover these paths.
The counted runner requires all four test files, all 20 named tests and all 27
assertion mutation receipts; compiler errors remain rejected as control success.

## Revised new-owner policy

The commissioning review changed the original shrink-only rule: a newly declared
nominal owner may introduce measured debt in its declaration commit. Existing
owners, including newly added Deserialize implementations on old declarations,
may never gain fingerprints. This changes admission policy, not the 115-owner
measurement above. The reviewed bootstrap is `266f305ac1642598cac948efb8ecba3466c66bb0`;
the ledger path is anchored, so rename-plus-enlarge cannot create a fresh baseline.
In the tealstreet-sdk mirror, where that monorepo commit is not an ancestor, the
anchor is the seed commit `91a50c7a2c14cc5e291c4b4f0633fde32f752757`, which restored
the ledger byte-identical to monorepo master `f769aa8323` after the #8006 re-root
import removed it. Gate authority moved here with tealstreet-next #8013.

Birth proof uses actual current compiler ownership plus historical nominal source
declarations, independently of active cfg and Deserialize. All historical source
blobs under the crate's src directory are examined. Same-name relocation and
delete/recreate are conservatively treated as an existing owner. External source
owners, historical path modules and opaque expansion cannot obtain an automatic
birth exception; they require explicit provenance support or strict decoders.
Source parsing is used only for declaration history, never to prove accepted shapes.

A sibling branch cannot lend its introduction to a delayed debt addition elsewhere.
Pre-introduction parents are neutral only after declaration absence is proved;
post-introduction repairs remain binding across merges. Controls cover a valid
new-owner addition and merge, dormant old declarations, delayed additions,
recreation, historical macros/path modules, renamed ledgers and sibling laundering.

### Expanded decoder discovery (reviewed repair)

The free-function/custom-trait review expanded the function census to 111 unique
candidates on the same production snapshot: 30 decoder functions and 81 explicit
non-decoder classifications. Native discovers 110; the additional wasm-only
candidate is the host logging function `console_warn`. These are **inventory and classification counts**,
not additional accepted-shape defects. Non-decoder classifications emit a
separate `classificationOnly` receipt and are excluded from the runtime-result
count. Their exact compiler signatures and generic bounds remain fail-closed.

The decoder adapters compare returned values, including fallback/null results,
rather than merely `is_ok()`. Presence wrappers retain `Some(null)` separately
from `None`. Numeric parsers tag NaN, infinities and negative zero before JSON can
collapse them. In-place normalization observes its mutated output. Large numeric
input witnesses cross the harness as raw JSON text so JavaScript cannot round
them before Rust parses them. Tooling parsers exercise tracked valid, malformed,
and schema-invalid files, with decoded fields observed rather than only counts.

Review also identified mutable-output functions, nested functions, local custom
traits and non-nominal trait receivers. Unsupported receiver/default/local-trait
surfaces fail closed. Existing nested helpers require explicit classifications
with a whole-source fingerprint; changing their source invalidates that review.
This fingerprint is supplemental discovery/classification evidence, never a
substitute for a production decoder's runtime result. Independent adversarial
review returned MERGE after the nested Visitor, inherited default-method and
mutable-output gaps were closed with named controls.

The final enforcing matrix passed with 499 receipts: 348 runtime receipts
(180 native, 168 wasm) and 151 classification-only receipts (80 native, 71 wasm).
These cover 180 unique runtime owners and 81 unique non-decoder classifications.
The exact outstanding set remains 115 owners and 1,136 fingerprints; the ledger
was not changed. All measured feature lists remain empty, with the limitation
described above. All 26 venue crates remain scaffolds on this snapshot.

The counted control runner passed all 24 tests and 43 assertion mutation receipts.
The 69 scoped Rust tests, 105 CI-wiring tests, formatting and strict Clippy passed.
Control files run serially to avoid racing their shared compiler/scanner fixture.
Compiler errors remain explicitly rejected as successful negative controls.

## Integration classification: nested array-index parser

After import-provenance repairs, integration discovery reached
`safe-cex-types/src/model_utils.rs::LOCAL_FUNCTION:index`. Its runtime owner is
`model_utils::entries`: direct ordered key/value outputs expose the helper's
canonical-index decision. Four input/output witnesses match live Node
`Object.entries(Object.fromEntries(input))`, including noncanonical numeric keys
and the maximum array-index boundary. These are output comparisons, not success
booleans. Independent review removed the canonical spelling filter; the actual
production runner compiled and reported a named function-output mismatch.
Restoring the source returned one runtime owner and zero failures. No production
function, historical census, or outstanding-debt entry changed in this repair.

The sibling `accounts.rs::LOCAL_FUNCTION:index` is linked to production
`normalize_numbers`. Five raw-JSON input/output-text witnesses retain ordering
through recursive objects and arrays; expected text was independently checked
against live Node. Review separately removed the canonical-key filter and
bypassed recursive object-value normalization. Both mutations compiled and
failed named output comparisons; restoring production source yielded one runtime
owner and zero failures. This classification adds no outstanding debt.

## Integration classification: canonical WS-auth visitors

The relocated request, payload and response visitors are linked to their actual
production decoders in `safe-cex-types::ws_auth`. Optional Deserialize output
witnesses now supplement the existing trait, concrete and alternate shape probes
before the same single owner receipt. A helper backed by a Deserialize owner
requires these output cases. The payload and seed retain direct Some/None
observations, and malformed-input TypeScript findings remain open.

Six scoped owners pass native and wasm execution. Independent review separately
made the actual request decoder consume and drop `id`, and the actual response
decoder discard a present-null `id`. Both compiled and failed named Deserialize
output assertions; restoring source returned two owners with zero failures.
The response function-local `Wire` is deliberately still unclassified: the
visitor classification does not classify a companion type by implication.

The counted control registry now contains 25 tests and 44 assertion mutations;
all passed with zero skips or TODOs. The added mutation preserves accepted shape
while decoding `buy` as `sell`, proving output checks add a distinct assertion.
All 183 CI wiring tests pass. This is new integration evidence, not a revision
to the historical 115-owner census or its 1,136 fingerprints. The outstanding
debt ledger remains byte-identical to the integration merge before these imports
and classifications (SHA-256
`ef262f617de63c26b325fb65a8ecefb3424862d534c91423cab6f7e53535286d`).


### Compiler-owned external reexports

The integration follow-up encountered four `safe-cex-num` reexport declarations
for two canonical `safe-cex-types::js_coercion` items: `JsCoercionError` and
`js_string`, each exported through `coercion` and the crate root. Diagnostics now
name the imported spelling, source path and compiler canonical path instead of
printing rustdoc's null item name.

Coverage follows actual Cargo compiler-artifact package IDs, not crate-name
coincidence. Selected-consumer runs recursively include the complete workspace
owner and ordinary production runtime/classification probes. Canonical declaration
path/kind must exist, and the consumer dependency's resolved features must equal
the independently compiled owner's features; mismatches fail closed rather than
claiming configuration equivalence. Public type aliases are resolved by matching
rustdoc compiler metadata paths to actual Cargo compiler-artifact filenames, then
checking the resulting package ID against workspace identities. Workspace-owned
aliases fail closed; genuinely external aliases pass; private aliases are not
public decoder boundaries.

A maintained two-crate fixture passes selected-consumer native and wasm runs.
A compiling `BUY` alias in its canonical `Side` fails the named production
Deserialize assertion; enabling a consumer-only dependency feature fails the
named compiler-feature mismatch assertion. Public aliases to a workspace type
and to a workspace package whose library crate is named `serde_json` each fail
at a named package-identity assertion. A public alias to registry
`serde_json::Value` and private aliases to both external and workspace types
pass. These add one counted test and two mutations, bringing the registry to 27
tests / 48 mutations.
This is discovery coverage, not a revised defect census. The historical 115,
89-removed / 26-surviving prediction and empty-feature baseline limitation remain
unchanged. The debt ledger is byte-identical throughout this repair.

Independent review added a fresh `Deserialize` record to the canonical dependency
and selected only the consumer package. Compilation succeeded, then the new owner
failed its named unclassified assertion; restoring the dependency restored the
passing run. Reviewer verdict: MERGE. This independently checks discovery rather
than replaying the author's widening mutation.

### Generated ICU4X provider source guards

The generated provider contains 14 `clippy::msrv = "1.88"` attributes, 103
`allow(unused_unsafe)` attributes, three `doc(hidden)` attributes, 149
constants, no macro invocations and no statics. The containing `mod data` is
private, so those constants are not public surface; the inventory must discover
them if that module becomes public. Only the `clippy` tool namespace is present
in the provider. The scanner accepts the exact `clippy::msrv = "1.88"` form
only in the generated provider file and accepts `rustfmt::skip` only on the
exact private path-module declaration. Built-in `allow` and `doc(hidden)` need
no exception.

The `rustfmt::skip` is load-bearing: removing it and running
`rustfmt --check --edition 2024 collation.rs` exited 1 with a 370-line diff that
reflows the pinned generated section. The provider, generator, profile and data
hashes remain unchanged. Replacing `locale!("und")` with
`Locale::UNKNOWN.into()` produced equal 165,422-pair results against Node
26.7.0 / ICU 78.3 before and after. The scanner rejects supported tool
attributes in ordinary source, wrong generated context/value, namespace
shadowing and unknown tool attributes; diagnostics name the rejected path.

The production matrix also found a function-local `macro_rules!` definition in
`safe-cex-store/src/store.rs`. This is out of public scope by lexical placement,
not by macro name: its function-local declaration and call pass discovery, while
the same macro hoisted to module scope fails with a named production assertion.
This changes discovery coverage, not decoder or debt classification.

### Mixed observed and typed APIs

An alternate-only owner may combine executed `observed` entrypoints with
explicitly classified `typed` entrypoints. Both retain the exact compiler API
list. A typed entry must pass the same raw-input role and signature checks as
an ordinary typed contract; a raw constructor, conversion, byte/string input,
or decoder bound cannot inherit permission from a neighboring typed method.
The owner must still execute at least one observed API. Generated dispatch
contains only observed methods, each with its own input/output witnesses.

This permits a native runtime type to classify an empty collection constructor
without inventing varying inputs or outputs for it, while executing its raw
string-key operations independently. The registered `observed typed entrypoint
raw compiler role` mutation removes that validation at the production branch;
its test requires an assertion failure and a restored passing baseline.
This format support does not waive a decoder, change the defect ledger, or
establish a complete production matrix result.


### Serde forwarding macro discovery

The qualified `serde::forward_to_deserialize_any!` identifier-only form is
reviewed against registry Serde 1.0.229 and its selected registry serde_core
1.0.229 provider. Both compiler artifact identities are required. Only the
reviewed method-name tokens are accepted; custom visitor syntax, unknown names,
extra token groups, imports under another name and shadowed namespaces remain
unsupported. Rustdoc still inventories every expanded method, and each method
retains its ordinary exact API classification and runtime obligations. This
permission changes source discovery only; it does not exempt deserializers.

The native typed-boundary observations use fixed visitors. Boolean forwarding
witnesses show the actual captured boolean reaches that visitor, including
methods whose names describe other Serde data types; they do not establish
numeric, string, aggregate or arbitrary visitor semantics. Enum and newtype
observations separately execute their concrete native branches.


The concrete `HostedSigner` tuple has a separate bounded compiler identity
probe: its source and expanded targets must be exactly an Rc of the native
RemoteSigner trait plus an optional String. Rc, Option and String each require
their resolved sysroot identity. An invariant native type equality checks the
actual alias against the independently authored tuple. This gives no permission
to generic tuples or other signer payloads. The registered expanded-target
mutation proves the compiler-target refusal remains active.

The active Serde rc feature makes `value::Str` (Rc<[u16]>) a real sequence
decoder. Its contract observes native UTF-16 units, including a surrogate pair
and a lone surrogate, and refuses strings, negative/out-of-range units and
nonintegral numbers. Numeric runtime observations project binary64 bits before
JSON so integer-valued floats cannot be confused with JSON integer storage.
CrossTradeStore now constrains its remote-derived decoder to object input;
its existing native observations and the focused object/array regression retain
the original map fields and reject positional arrays. No debt entry was added.


## Per-crate contract storage cutover

At source base `71107baba8`, the former
`contracts.json` held 797 contracts. Those exact entries now occupy 16 compiler
crate files in `contracts/`; no row, native adapter, signature, expected value or
classification changed. `accepted-contract-files.mjs` supplies the complete
merged object to the gate, local helper review, native helper controls and
proposal helper. Existing explicit JSON fixture files remain supported.

The executed test compared the COMPLETE original production object with the
new loader using deep strict equality, including every adapter string and
ordered array. Their recursively key-sorted semantic JSON SHA-256 both equal
`b396f4a835ef661ae97f65ad883a2e64b4ba4aa21bdab8b4ba09f21c69a2eab9`.
On this cutover commit, reproduce the comparison from Git history as follows
(later reviewed contract additions naturally change the complete live set):

```sh
git show 71107baba8:sdk-rs/safe-cex-parity/accepted-set/contracts.json > /tmp/accepted-contracts-before-cutover.json
node sdk-rs/safe-cex-parity/scripts/accepted-contract-files.test.mjs --legacy-contracts=/tmp/accepted-contracts-before-cutover.json
```

The permanent legacy fixture also checks exact values, adapter newlines,
Unicode, negative zero and explicit-file compatibility. New directory controls
refuse missing/malformed/empty inputs, misplaced or duplicated owners,
unexpected files and symlink entries. Removing the namespace check or dropping
an entry from the loaded set each fails its named assertion; restoration passes.
The registered controls retain all earlier tests and mutations: 123 tests and
144 mutations. Only the three new focused tests and two new mutations were
executed for this storage change; the long full control suite was not repeated.

All 23 focused loader/policy/local-helper/proposal tests and all 368 wiring tests
pass. Fresh ordinary OKX compiler discovery remains 145 owners/four aliases;
24 existing selected production witnesses loaded through the new directory pass
with zero failed owners. Independently running the ordinary gate on the existing
isolated fixture with a JSON file and with its per-crate directory produces
exactly equal six native receipts (five runtime, one classification), zero
failed owners in both runs. This proves loader compatibility, not workspace
contract completion or any change to the standing ratchet.

## GRVT fixed compiler alias identities

Eleven finite entries in `generic-aliases.json` pin the actual GRVT declarations,
including their original generic defaults and lifetimes. The selected arguments
are explicit bool/bool pairs or bool, with static lifetimes for the three
unsized FnMut aliases. Source defaults are neither deleted nor silently used as
broader coverage. Every existing registry entry remains unchanged; the reviewed
alias implementation and its caller, source, compiler-target and root guards
are unchanged.

Fresh pinned native/default compiler discovery on source base `71107baba8`
reports 182 owners, 19 aliases and three construction obligations. All eleven
new entries match that compiler's raw/expanded targets and original generics,
and their roots are authenticated against its actual selected Cargo artifacts.
Ordinary production policy routing chooses exactly the independently authored
expected Rust types. One metadata-only native Rust compilation proves invariant
type equality for all eleven aliases against the actual selected GRVT rmeta,
with warnings denied; three unsized aliases use invariant function/pointer
PhantomData. This is compiler identity evidence, not runtime decoder proof.

All 21 focused reviewed/identity/callable/alias policy regressions pass. The
actual eleven-entry compiler preflight additionally rejects changed source,
raw/expanded targets, generics, caller redirects, missing Serde origins and
unknown registry names. There is no new general generic-alias permission.

AddedMarkets<bool> and JsResult<bool,bool> remain real decoders whose contracts
and native value witnesses belong to the consumer review. Fields<bool> retains
`Rc<RefCell<IndexMap<&'static str,bool>>>`: its static Deserialize bound compiles,
and its DeserializeOwned bound is rejected as not general enough. The ordinary
owned alias emitter therefore still refuses this owner; explicitly selecting
the borrowed emitter compiles and hits its existing runtime refusal. Neither
refusal is a passing native witness. Exact borrowed production-path support is
separate work; no keys, dependencies, classification, runtime guard or standing
debt changes in this registry addition.


## Transport Result-bearing future identities

Two separate finite entries in `generic-aliases.json` review
`safe_cex_transport::provider::TransportFuture<'static,bool>` and
`safe_cex_transport::provider::RestFuture<'static,bool>`. Their actual source
outputs are `Result<T,TransportError>` and `Result<T,RestFailure>`; neither is
the original local-future family with `Output=T`. Every prior registry entry
is unchanged, and no shared helper or guard changes.

Fresh Rust 1.99 native/default rustdoc plus selected Cargo check artifacts on
source base `54537b8d0a` discover 108 transport owners, six aliases and nine
construction obligations. Both declarations retain exact raw/expanded ASTs,
original lifetime/type parameters and the authenticated Pin/Box/Future/Result
sysroot roots. The existing reviewed alias path emits invariant equality
against independently authored native types containing the actual local
TransportError/RestFailure. Actual production alias probes without cfg(test)
execute ordinary Serde and conversion checks: two explicit classifications,
zero failed owners. These focused temporary control contracts establish the
fixed capability identities; they are not installed production classifications
or evidence of future polling or transport execution.

All 24 focused reviewed/identity/callable/future/alias policy tests pass.
Against the fresh compiler aliases, changed source, raw/expanded target, generic
default, caller expected-type redirection, missing root identity, missing
serde_json origin and unknown names each retain their existing refusal.
There is no permission for arbitrary Result-bearing futures or generic arguments.

The ordinary transport inventory now passes both future registrations and stops
at the separate `safe_cex_transport::secondary::RateLimitWrapper` declaration:
its lifetime-only generic target requires another exact compiler review. No
whole-package or workspace gate clearance is claimed here.


## Transport rate-limit wrapper identity

One finite registry entry reviews the actual
`safe_cex_transport::secondary::RateLimitWrapper<'static>` alias on source base
`4c2a8c0962`. Its raw and expanded compiler targets are the unsized
`dyn Fn(Arc<dyn RestAdapter>)->Arc<dyn RestAdapter>+'a`, with one original
unbounded lifetime parameter and no type substitution. Fresh pinned native
rustdoc/check discovery retains 108 owners, six aliases and nine construction
obligations; the selected Fn and repeated Arc roots authenticate against the
actual sysroot artifacts. The independently authored native expected type
retains the actual local RestAdapter trait in both positions. Existing registry
entries and shared helper guards remain unchanged.

Actual production alias probes without cfg(test) pass with one explicit
classification and zero failed owners. The native emitter requires invariant
function/pointer equality for this unsized type and runs its ordinary Serde and
conversion checks; no callback invocation is claimed. All 24 focused alias
policy regressions pass. Fresh compiler preflight also retains seven refusals
for changed source/raw target/expanded target/generic bounds, caller redirect,
missing serde_json origin and unknown owner. The focused temporary contract
is not an installed production classification.

## Exact inherited constant-method witnesses

`inherited-constant-outputs/safe-cex-venue-grvt.json` separately reviews four actual indexed GRVT
Exchange methods: fetch_symbol_max_leverage, change_position_mode,
change_margin_type and fetch_public_trades. Each inherits the maintained TS
BaseExchange rejection. The finite policy pins the complete normalized compiler
API, actual package/manifest, raw-method role, Rust method span and source hash,
TS base method lines/excerpt and whole-file hash, venue inheritance source hash,
native binding fixture hash, and exact adapter/cases. Unreviewed methods cannot
use the flag, and no unit-input or omitted-entry exception is added. The two
existing Num serializer constants and all their guards remain unchanged.

Distinct real inputs call the actual production Exchange methods and observe
native Error name/message, HTTP count, Store balance bits/orders/positions/markets,
notifications, socket count, logs, reports and clock state. No input echo stands
in for native output. The complete control fixture retains all eleven compiler
APIs, including the ordinary typed constructor and six independently reviewed
nonconstant adapters. Its selected owner receipt is separate from any complete
GRVT package or matrix claim; production classifications remain consumer-owned.

The native control uses fresh pinned rustdoc discovery, then instruments a
byte-preserving copy of the production library without cfg(test). Two separate
post-validation emitter mutations alter the four native error-name observations
or HTTP-count observations. Each must compile, produce eight named output
mismatches for this actual owner, and fail the native witness assertion; restored
execution must pass. Compiler or discovery refusals cannot count as controls.
Three additional executed pure-policy mutations remove the maintained-source,
fixture or adapter pin and must fail their named refusal assertion. The control
registry retains every old control and adds three tests/five mutations, for
126 tests and 149 mutations. The long complete control suite is not rerun here.

Focused execution passes all three new tests and five executed mutations:
the native test completes in 171 seconds with the complete eleven-API owner,
eight expected mismatches for each compiled mutation, then restored zero
failures. The 15 new/existing contract-policy tests, all 368 wiring tests and
parity checking (7,459 rows / 17,198 public Rust items) pass. No whole-package
GRVT, wasm or full accepted-set matrix clearance is claimed by this policy PR.
