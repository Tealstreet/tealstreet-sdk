# Parity tooling

The behavioral reference remains [safe-cex/CLAUDE.md](../../../packages/safe-cex/CLAUDE.md). The manifest contract is per TS source file: `[file].ts` and `[[symbol]]` with `ts`, `kind`, `layer`, `rs`, `status`, `note`. Enum declarations have rows; their members do not.

**Pending is valid.** Empty `rs`, `layer`, and `note` are the untouched baseline. `pending` requires empty `rs` and cannot discharge invention. `divergent` requires a nonempty mapping (string or array); `divergent` and `not-applicable` require a nonblank note; `not-applicable` also requires empty `rs`. Layer is required exactly when `rs` is nonempty, regardless of status. Every supplied mapping must resolve. `rs` accepts a string or an array of strings; array members must be distinct, arrays require a note explaining the split, and every member is resolved and counted. When multiple ported rows share an item, every sharing row requires a nonblank note. Reports count rows once per represented crate and distinct non-pending Rust items per file, directory, crate, and total; cross-crate splits count only that crate's items in its distinct count. `parity:report` and the library `report` require a Rust index and pass the same status/invention checks as `parity:check` before producing counts; invalid claims emit diagnostics and no progress table. The output labels these as validated manifest claims, explicitly excluding proof of behavioral parity. Do not turn status validation into a port-completion gate.

**Mapped names must correspond or explicitly name their rename target.** Both `ported` and `divergent` mappings retain the TS qualifiers and transform callable leaves to snake case (types/classes/enums/exported value constants retain their name; constructors use `new`; static/accessor/private member markers are stripped, including legacy `Class#member`). A rename requires the complete `crate/file::Item` target as a standalone token in the note; backticks and surrounding prose punctuation are accepted. A split/shared-claim explanation does not implicitly authorize a rename. Array targets follow the same correspondence rule individually; tests reject mismatched single- and multi-item arrays with general notes and explicitly exercise the per-member guard. An array note must contain explanation text beyond its mapping-target tokens; bare targets (including backticks/punctuation) cannot discharge the split requirement. This checks separation of declarations and explanation, not prose substance. Notes still explain splits and shared claims, and their substantive truth remains a review obligation. Hardening fixtures cover same-leaf methods on different classes and noted renames.

**Source provenance is declared per manifest.** Optional `[file].rust_files` lists exact allowed `crate/path.rs` files alongside `ts`, independently of symbol mappings. Every `ported` or `divergent` target must belong to that source's declaration, including arrays; rename notes cannot override provenance. Missing declarations fail mapped rows, while untouched pending files need none. Every declared file must be used by at least one ported or divergent row in that manifest; unused allowances fail with the source and unused Rust file named. This exposes broad supersets without forbidding legitimate multi-file splits; it does not prove that an association with a used file is semantically correct. Wildcards, parent traversal, item paths and duplicate targets are rejected. Declare adapted consolidations and cross-crate splits deliberately in the owning manifest; no shared central registry is used.

**Discovery is syntactic, across target configurations.** Library/binary roots and their declared module graphs are indexed; exact `#[cfg(test)]` branches are omitted. Other cfg branches remain in the index so host-only and wasm-only items are both covered. Unrestricted `pub` declarations require mappings even inside private modules. Private and restricted declarations can resolve mappings but do not trigger invention. Inline modules add namespaces; external modules use their actual source file in `rs`.

**Structural exclusions** are module declarations, proven intra-workspace reexports, struct fields, enum variants, trait impl members, and implicit derive output. Their owning type/trait declarations remain gated. Checked-in generated and FFI declarations need exact `[[exempt]]` entries (`rs` plus nonblank `reason`) in `sdk-rs/parity/exempt.toml`; extra behavior needs a divergent row instead. The FFI boundary probe and version exports have exact glue exemptions; other public declarations still require mappings or justified exemptions.

**Public reexports** of dependencies are declarations at the exported alias (`crate/path::Alias`), including grouped imports and nested modules. Reexports proven to target a direct workspace declaration remain structural; workspace dependencies are recognized by their member path, including dependency aliases and custom library names. Other named aliases remain gated conservatively, including private-import indirection through `crate`, `self`, or `super`. Public globs with an external or unresolved origin fail loudly; name the exports explicitly. A leading `::` uses the dependency namespace and cannot be hidden by a local module. Declaration proofs are isolated per library/binary root, and imports sharing a declaration's path defeat that proof across cfg branches. Globs importing uncertain public aliases propagate that uncertainty to their importing scope, including private globs through local bridge modules. Conflicting dependency ownership across target configurations fails explicitly rather than selecting one configuration's origin.

**Fail closed on unsupported syntax.** Only unshadowed `compile_error!` is allowlisted as a diagnostic that cannot declare items. Additions require proof that the macro cannot declare an item. Any same-name binding across indexed roots, unresolved glob, or `#[macro_use]` (including nested `cfg_attr`) makes that diagnostic origin uncertain and fails conservatively. Other item macro expansions/includes, cfg_attr module paths, workspace globs and qualified inherent impl owners remain unsupported. Imported inherent owners require one unconditional direct same-crate struct/enum/union origin, including unconditional file/module ancestry; conditional, duplicate, indirect and external origins fail closed. A local unconditional nominal beats glob imports but never an explicit same-name import. Methods retain their impl-site source targets and invention checks. Private singleton thread-local storage lives inside its sole accessor; module-level thread_local expansions remain rejected. This tool does not pretend to be the Rust compiler or expand arbitrary code. Add support and regression fixtures before introducing those forms in SDK declarations.

Run from `sdk-rs`: `cargo fmt --check`, `cargo clippy -p safe-cex-parity --all-targets -- -D warnings`, `cargo test -p safe-cex-parity`, `cargo build -p safe-cex-parity --locked`. Both binaries accept `--workspace`, `--manifest`, and `--exempt`; defaults select this SDK's parity data. Missing/empty manifest directories fail rather than reporting a false green. Test data lives inside this crate's `fixtures/`; `clean` maps a real public enum and `pending` preserves the untouched all-pending corpus; the production manifest and TS extractor belong to their separate owner.

Root scripts `parity:extract` and `parity:extract:check` invoke the separately owned `sdk-rs/parity/extract-ts/src/cli.ts` (`node --import tsx`; check adds `--check`), after a regular-file precondition that reports the missing extractor clearly. `parity:check` and `parity:report` keep the repository root as the working directory, so forwarded fixture paths are repo-relative. The coverage gate must be verified with the TS extractor after it lands; these Rust tests verify invention and status only.

For diagnostic triage, `node sdk-rs/parity/scripts/diagnostic-census.mjs <log>`
groups direct failing checker output without changing validation. Unknown output
or an empty input fails rather than silently losing a diagnostic. Its focused
tests run with `node --test sdk-rs/parity/scripts/diagnostic-census.test.mjs`.

Representative production metadata checks run with `node --import tsx --test
sdk-rs/parity/scripts/manifest-repairs.test.mjs`. They decode actual manifests
before invoking the existing maintained checker binary; build that binary first
if absent. These selected-source assertions do not replace the global gate.
`node --import tsx sdk-rs/parity/scripts/manifest-repair-controls.mjs <control>`
proves each declaration repair against both native metadata and that checker.
Controls restore exact bytes and rerun the focused target without Cargo.
Controls require input identical to HEAD, including staged content, so the
reported revision names the actual metadata being measured.

`parity-trait-inventory <sdk-workspace>` observes trait owners and members with
syn, filtered by the maintained public index. It supplies structural evidence
for diagnostic triage; it changes no mapping, exemption or validation rule.
The CrossTrade production-source regression indexes all five sibling files,
asserts three native loop declarations, then verifies invention still gates them.
Explicit test-only AST filtering precedes that intersection: a test trait can
share an ID with a production struct, so name membership alone is insufficient.
`parity/scripts/diagnostic-causes.py` joins that inventory to direct diagnostics
and decoded manifests. Its disjoint structural groups are candidate repair
families, not automatic rename, split or provenance authorizations.

`parity-check --invention-crate <package>` requires explicit `--manifest` and
`--exempt` inputs. It builds the complete maintained workspace index, preserving
canonical local/shared reexport resolution and every mapping target, then narrows
only public invention obligations to that package. Status, target resolution and
source-provenance checks still apply to every supplied row. Global/default gates
are unchanged; report/progress do not accept this scope. The split-mapping CLI
fixture also requires unmapped-item and missing-provenance failures and restoration.

**Progress shares report counters.** `parity:progress` builds the same source
index and calls validated `report`, then formats its total, phase buckets and
ten largest pending source files. It does not count symbols itself. Phase
buckets allocate each source file once according to the plan: public WS and
candle-cache paths P3, tier-1 families P4, other venue files P5, consumers P6,
transport/signing P2 and remaining pure/core files P1. Empty P0/P7/P8/P9 buckets
mean no scoped TS rows, not completed work. These are file allocations rather
than claims that overlapping phase behavior or runtime-host milestones are done.

Progress validates the exact report header before interpreting positional status
columns; changed or additional columns fail closed. Mixed-status fixtures make
ported rows, claimed Rust items and ported-plus-divergent distinct, including
not-applicable rows and array mappings, and check the pending remainder.

**Accepted-set enforcement** is `yarn parity:accepted`; its assertion controls
are `yarn parity:accepted:controls`. The compiler inventory uses pinned rustdoc
JSON with private/hidden items and actual Serde trait identity, including manual
impls. It separately inventories conversions, public Self-returning
constructors, free-function decoder candidates and custom trait methods, even
without Deserialize. Raw-input functions remain candidates when they return unit
and mutate an output argument. Generic bounds and local aliases participate in
discovery. Unsupported trait receivers and uninstantiated decoder defaults fail
closed unless `../accepted-set/receiver-traits/<crate>.json` reviews them against a
whole-source pin (`non-decoder` or `delegated` only). `PatchSource` counts as raw input. Owner-local compiler trait probes cover
block-local declarations rustdoc omits. Nested functions and local Visitor/Seed
helpers require explicit whole-source fingerprints; decoder helpers also name
an inventoried production decoder whose probes execute. Deserialize decoder helpers additionally require direct input/output witnesses
on their actual production owner. Those observations supplement both existing
shape paths and share the same owner receipt; they cannot replace shape checks.
These classifications are not independent runtime proof. Unknown local custom traits fail closed. Local decoder implementations must move
to module scope for concrete API classification; local unions/generics and
unknown procedural expansion fail closed. Function-local `macro_rules!`
definitions and their invocations are ignored by public-surface discovery
because lexical block scope makes them unnameable outside that body. Their
invoked effects are checked separately: exactly one empty matcher or one
`($name:literal, $value:expr)` matcher is supported, with no extra arms,
repetitions, forwarding fragments or unknown captures. The census substitutes
parenthesized fragments into the body and visits both that body and every
argument, including unused arguments. Hidden impls, modules/imports, opaque
attributes and unresolved nested macros remain refused; no macro-name allowance
is involved. The actual Bitrue enum registration recipe and compiled global-impl
controls exercise this bounded form.

Direct workspace dependency-root globs can supply exact public names to these
same import guards. The driver authenticates the active Cargo binding, consumer
rustdoc dependency, unique library artifact and feature set; it then builds fresh
private/hidden rustdoc for that dependency under the same target and features.
Complete public root exports, including named reexports and macros, are checked
against the actual library source path. The existing effect scanner also checks
that dependency's complete guarded source closure (including inactive modules),
using fresh provider artifacts and macro origins. Unverified provider expansion
cannot hide a non-doc export behind rustdoc's configuration. Unresolved public glob reexports,
missing/ambiguous artifacts or export names, and competing source root bindings
remain refused. The scanner expands proven names into ordinary import checks;
an exported builtin-name macro or attribute does not acquire a trusted origin.
The Bitrue control compiles and executes a dependency `format!` through the
actual exchange module's glob, requires its existing import-scope refusal, then
restores both sources exactly and reruns the full inventory. A second compiled
control checks an export hidden behind a module macro and `cfg(not(doc))`. No dependency-name
allowance replaces this proof. Registry/sysroot macro origins keep their existing
separate authentication.

A module-scope `macro_rules!` is accepted only when every same-name definition in
the target is unconditional and every arm transcribes without a declaration
keyword or `item` fragment; each invocation's arguments get the same check. Macro binding guards still
reject local shadowing of trusted expression/derive/attribute names and
namespaces. A target-wide token proof permits module-level `core` declarations only when
every `core` identifier in every supplied source is another such declaration
or an AST-proven value binding/reference, and the actual compiler `core` origin
is authenticated. Only authenticated std `vec!` operands are parsed as expressions;
other macro tokens remain opaque. Paths, imports, aliases,
extern declarations, block-local modules, macro tokens and inactive cfg tokens
revoke that proof. Every other source finding remains enforced.
A separate
source module walk rejects doc-dependent cfg predicates before rustdoc, including
external-module inner attributes and raw identifiers; block-local external modules
fail closed. The control runner pins test files, named tests and executed mutation
receipts, so deleting or skipping a control cannot shrink the suite silently.

Probes instrument byte-preserving repository copies, including path-shared
`infra/tradetunnel` sources. Native probes execute production libraries without
`cfg(test)`; wasm probes execute the actual compiled wasm in Node. Each owner
uses both its trait and concrete qualified decoder. Public raw-input adapters
need positive witnesses and forbidden-input probes. Compiler failures never
count as mutation-control success. `--inventory`, `--census`, and `--measure`
are diagnostics; only the matrix command enforces the standing ratchet.

Classify new owners in `../accepted-set/contracts/<compiler-crate>.json` with TS/tooling evidence,
shape, positive JSON fixtures, and exact compiler entrypoint signatures. Free
functions have explicit decoder/non-decoder classifications. Decoder adapters
observe actual outputs, preserving fallback results and tagged non-finite values;
non-decoder receipts are counted separately. Total decoders cannot be validated
by an is_ok-only adapter. A new
raw decoder needs an adapter and positive witnesses; a typed constructor needs
its reason. Record arrays use Serde's field order, not JSON map insertion order.
Observed methods normally require distinct inputs and outputs. The two
`Capture` serializer metadata constructors in `../accepted-set/constant-outputs/<crate>.json`
and exact reviewed venue inherited refusals in `../accepted-set/inherited-constant-outputs/<crate>.json`
may set `constantOutput: true`. Only individually installed GRVT, BreakoutProp,
Bitget and the three exact Kucoin inherited-refusal policy records qualify; adding a package name alone grants no method
permission. Their actual compiler package, API, body span,
whole-source hash, native adapter and cases are pinned. Inherited refusals also
pin the maintained TS base method lines, base/venue file hashes and native
harness bytes. Distinct genuine inputs still execute and observe native Error
identity/message, zero HTTP and unchanged Store/notification/runtime state.
Every compiler-listed API remains in the complete owner contract; get_account
and schedule_cancel behavior tests do not invent indexed entries or unit-input
permissions. Unknown constant methods and changed adapters remain refused.
The single reviewed `DirectTransport::ws_connection` native-method record in
`constant-outputs/safe-cex-transport.json` also pins its actual compiler API,
package, raw-input role, Rust span/hash, maintained TS callback and exact native
adapter/cases. Distinct URL inputs observe the returned connection capability;
both reviewed outputs are absent. No other transport method inherits permission.
Three reviewed Kucoin Spot overrides may use native-method records for constant
leverage `1` and unit-returning mode changes. They require the exact maintained
TS override body and source hash, genuine native result/state witnesses and both
executable fixture hashes. These overrides receive no inherited-error permission.
Kucoin inherited refusals also require both the I/O harness and observer helper
hashes. Missing reviewed records remain refused; package eligibility installs no
contract or permission by itself.
The bounded `construction/<crate>.json` policy classifies Clone/Copy on private-field
types, including service handles whose shared ownership is intentional.

`outstanding-defects.json` records owed defects with crate and agent owners,
never permissions. Exact owner/target/features/failure sets must match runtime
measurement. Repairs
remove fingerprints/entries in the same commit. Existing owners may never gain
or regain failures. New debt requires a compiler-confirmed nominal declaration
introduced in that same commit, with a repository-relative `declaration` source.
The ledger path and reviewed bootstrap commit are anchored; renames require an
explicit reviewed migration. Historical source parsing establishes declaration
birth independently of Deserialize and cfg activation. Same-name relocation or
recreation is conservatively rejected; opaque historical macros/attributes and
external path modules cannot authorize a new-debt exception. Runtime probes still
establish accepted shapes. Merge checks retain every parent repair. Shape inventory and defect
counts are separate; see `../accepted-set/EVIDENCE.md` for the immutable first
census and subsequent measurement corrections. No classification or invalid
positive fixture can be grandfathered as shape debt.

Public external reexports resolve through rustdoc's canonical item path and
Cargo compiler-artifact package identity. A scoped run includes the complete
workspace owner and its runtime probes; matching a crate spelling alone cannot
prove ownership. Consumer dependency features must equal the standalone owner's
actual compiler features, otherwise measurement fails closed. Unresolved public
uses and external type aliases fail by name. This does not authorize third-party
ownership or silently choose a different dependency configuration.

Trait/API annotation checks run with `node --import tsx --test
sdk-rs/parity/scripts/trait-declarations.test.mjs` after building `parity-check`
and `parity-trait-inventory`. Literal family floors and consuming-operation
witnesses are independent of decoded manifest arrays; the AST inventory supplies
exact member identities. `trait-declaration-controls.mjs [first-family-index]`
removes real owner mappings in one batch, requires named native failures plus
invention diagnostics, restores exact bytes and requires the focused tests to
pass. A second batch falsely claims explicitly unassigned capabilities and must
fail both native ownership and canonical-remainder assertions. These are
annotation checks, not fresh runtime behavior proofs.

Two exact nongeneric function-pointer aliases (`RetryCondition` in Bitunix and
`BlofinApiFactory` in Blofin) have reviewed compiler signatures in
`../accepted-set/function-pointer-aliases/<crate>.json`. Their source identity, complete
raw/expanded ASTs, every nested authenticated compiler root and selected workspace
package are pinned. Native probes require invariant equality with independently
authored Rust types and execute the ordinary alias Serde/conversion checks. This
is capability classification, not callback execution or permission for other
function pointers. Contracts cannot supply expected signatures or probe modes.

The concrete WEEX `WeexKlineRow` protocol tuple has its own finite policy in
`../scripts/accepted-weex-kline-alias.mjs`: all eleven ordered raw/expanded slots,
eight repeated authenticated String roots and the sized invariant Rust type are
pinned. Its native control observes every decoded slot and executes the ordinary
Serde/conversion probes. Expanded-slot, native-identity and removed-policy
controls must fail before restoration passes. This policy grants no permission
to other tuples; callers cannot redirect its expected type, suffix or probe mode.


Accepted contracts now live in one JSON file per compiler crate under
`../accepted-set/contracts/`, for example `safe-cex-venue-okx.json` for
`safe_cex_venue_okx::...`. The gate and review helper both use
`scripts/accepted-contract-files.mjs`; imports needing the whole reviewed set
must call `loadContracts()` rather than read a former monolithic file. Binary
crate namespaces have their own files too. Directory loading rejects misplaced
or duplicate owners, empty shards, unexpected files and symlink entries.
Explicit `--contracts=<file.json>` fixture inputs remain supported; a directory
is also accepted. Storage changes no contract classification or native proof.

Eleven GRVT aliases use the existing reviewed fixed-instantiation registry in
`../accepted-set/generic-aliases/<crate>.json`. Each pins its exact source, raw/expanded
compiler targets, original generics (including defaults/lifetimes), canonical
root and independently authored invariant Rust type. The selected arguments
are explicit bool or bool/bool, and static lifetimes for three unsized FnMut
capabilities; these scopes do not classify other instantiations or silently
select a source generic default. AddedMarkets<bool> and JsResult<bool,bool> are
actual decoders, not callback exemptions. Fields<bool> preserves its original
static borrowed string keys: its reviewed type identity does not grant
DeserializeOwned or bypass the existing borrowed-alias refusal. Decoder
classifications and borrowed production-path coverage remain separate obligations.


The transport provider's `TransportFuture<'static,bool>` and
`RestFuture<'static,bool>` have separate finite identities in
`../accepted-set/generic-aliases/<crate>.json`. Their exact raw/expanded compiler ASTs
retain `Result<bool,TransportError>` and `Result<bool,RestFailure>` respectively,
including the original lifetime/type parameters. The existing reviewed alias
path emits invariant equality against independently authored native Rust types
and runs ordinary Serde/conversion checks. This establishes the selected
capability identities, not future polling, transport execution, other generic
arguments or a decoder exemption; ordinary contract review remains required.


The separate `secondary::RateLimitWrapper<'static>` registry entry retains its
actual unsized `dyn Fn(Arc<dyn RestAdapter>)->Arc<dyn RestAdapter>` target. Its
existing unsized alias path requires invariant native function/pointer type
equality and ordinary Serde/conversion checks. The finite static capability
identity does not execute its callback or classify other lifetimes.

The Num output macro provider proof returns the exact `macro_export` attribute
span only after validating its unconditional source route, canonical definition,
export syntax and recipe. The driver covers that one attribute event alongside
the existing exact definition event. The scope scanner still reports every
export attribute; an unrelated exported macro in the same file remains refused.
The registered numeric-output controls test both retained coverage and nearby
position refusal. Provider native observations retain the parsed attribute span.

Reviewed policy maps are split per crate under `../accepted-set/<table>/` and
loaded through `scripts/accepted-policy-files.mjs::loadPolicy`. Existing compiler,
source-pin and native evidence checks receive the same merged records. Keep the
flat JSON maps deleted when merging older branches. See
[`POLICY-FILES.md`](../accepted-set/POLICY-FILES.md) for table names, ownership,
migration and complete cutover equality proof.


The one exact GRVT `composition::Fields<bool>` borrowed map has a separate
`shape: "borrowed-map"` route contract with the complete `borrowedRoutes`
matrix from `scripts/accepted-grvt-fields.mjs`. Its existing fixed compiler
identity is retained. Twelve immutable JSON texts each execute static text,
static borrowed Value, and owned Value admission separately. Actual ordered
keys and bool payloads are observed; text keys must retain their exact offsets
and borrowed Value keys must point into the retained Value, outside the source
text. Escaped text keys and nonempty owned Values refuse; an owned empty map
admits. Every ordinary alternate trait detector still runs. The general borrowed
AliasProbe guard and DeserializeOwned bound remain unchanged. The finite route
policy cannot classify another alias, type argument, target, package or adapter;
a reviewed consumer contract and all 36 route witnesses are still required.

Six Hyperliquid Value/Error aliases retain their actual associated projections
`<H as WsPingHost>::Value/Error`. Their finite generic-policy records select only
`Realm` and independently expect its concrete `realm::value::Value`. Discovery
authenticates the same-crate trait, Realm and Value compiler roots, the unique
Realm implementation, its ownership links and actual associated binding before
normalizing compiler IDs. The route checks exact raw/expanded targets, unbounded
host parameters, source files and selected workspace package. Contracts cannot
redirect the host or expected type. Native invariant equality and all ordinary
Serde/conversion detectors remain required; these capability identities do not
execute callbacks or classify other host instantiations. The compiled control
renames only the copied concrete value owner while retaining its Value alias;
all six native owner-identity receipts must fail before restoration passes.

The local source-scope helper policy pins both accepted_source.rs and the
accepted-source binary's actual complete file bytes. The bounded local macro
substituter and authenticated-glob UseTree resolver are syntax transformations;
the binary's name/use collectors and Bindings visitor inspect parsed syntax.
Their specific non-decoder rows do not waive macro-effect, provenance, import
shadowing or missing-classification checks. Refresh these reviewed file pins
alongside changes to either source file.

The `JsJsonValue::from(Value)` native observer uses that enum's actual `String`
object keys directly; retained-input UTF-16 key adapters belong to their separate
owners. `node --test safe-cex-parity/scripts/accepted-string-key-observer-native.test.mjs`
(from `sdk-rs`) runs the canonical compiler inventory and generated production
probe, including nested Unicode/NUL/empty keys and a compiled wrong-key output
control. No classification receipt can replace the executed conversion witness.

Kraken's exact `stream_updates::NumericChanges = Vec<(&'static str, f64)>`
uses the separate reviewed `borrowed-sequence` route policy in
`scripts/accepted-kraken-numeric-changes.mjs`. It authenticates the source span,
raw/expanded compiler AST, sysroot Vec artifact and invariant native type.
Its complete four-route matrix observes native f64 bits and borrowed key pointers
or offsets, including duplicate keys, escaped text refusal and owned nonempty
Value refusal. The general borrowed-alias refusal and DeserializeOwned bound
stay intact; a control bypassing the exact native routes must explicitly select
that unchanged borrowed guard and produce its compiled runtime refusal. Do not extend this permission to another owner or shape by copying its
contract. Registered native identity/member-order/dispatch controls must fail by
runtime receipt and pass after exact restoration.
