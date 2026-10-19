# Orderly venue port

`classes::construct` is the constructor used by the production binding and the
source-oracle class tests. Test-only convenience constructor wrappers were removed.
`scripts/classes-controls.py` checks normalization and all family tags against
those class tests and the real Exchange/signing binding, restoring exact source.

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/orderly/CLAUDE.md).

`types.rs` mirrors the ten runtime constants from `orderly.types.ts`. Ordered
entry arrays preserve own-key enumeration; consumers must use shared prototype-aware
lookup when reproducing dynamic JavaScript property reads. Those consumers remain pending. The five wire types live in `wire.rs`, with
private remote-serde companions and shared numeric ingress at every object boundary. `utils.rs` mirrors both symbol function bodies;
manifest notes disclose the shared UTF-8 boundary and omitted mutable lodash cache.

`tests/source-oracle.cjs` transpiles and executes actual repository TypeScript,
including shared enums and logging enum, with real lodash from beside the compiler.
No replacement implementations or frozen expected-output fixtures are used.
`source_parity` directly compares each constant and each direction of symbol conversion,
records the timeout's type/bits before JSON, and witnesses surrogate/cache boundaries.
Set `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

Scope cargo commands to `-p safe-cex-venue-orderly --jobs 1`. During development
use cargo check or a single scoped test target; reserve the full scoped suite,
fmt check and separate strict host/wasm clippy for merge-up readiness. Never run
two Cargo processes concurrently. The concrete `native_exchange::OrderlyExchange` composes the existing source bodies
on shared `js_runtime` primitives; keep source workflows in their owning modules.

`tests/controls.py` mutates one production constant/helper or one oracle capture
at a time and requires the named assertion to fail, restores exact bytes in a
finally block, then requires a passing run. Do not run concurrently with other
gates. `tests/check_manifest.py` runs the production parity checker against a
temporary one-member workspace pointing to this actual crate and its actual
manifest directory, with no exemptions; it does not validate other venues.

`tests/wire-oracle.cjs` uses the live TypeScript compiler to enumerate the five
actual source aliases (including nested records and literal unions), then runs
the transpiled actual source with typed capture wrappers. Type aliases erase;
the compiler-derived shape is the structural witness, not runtime validation.
The permanent inherited-required-field control uses TypeChecker properties,
not a members-only walk. One Node process returns the complete corpus. Rust
compares decoded native fields before serialization on raw, owned and borrowed
numeric ingress; same-type field values differ. Optional missing fields remain
absent and present null is refused. Shared representation boundaries are cited
in each type's manifest row; no shared fixtures are modified.

`tests/wire_controls.py` proves five symmetric production field-name swaps fail
the decoded-native assertions, then checks serializer omission, tagged literal
objects, nested optional null, nested/array-element positional records, inherited
fields and signed zero. It always restores original bytes and requires passing
restorations. `ORDERLY_CONTROL_START` can resume a failed control run at its
zero-based index; the printed count is only the controls actually rerun. Literal
unions use private remote enum metadata with public string-only deserializers.
Wire witnesses also compare JS serialization separately from native field/bit
observations, and omit each optional field independently while keeping ancestors.

`encryption.rs` ports the SHA512 callback, base64 character replacement and
secret parsing/signature encoding. Ed25519 uses the shared crypto signer. The
manifest discloses UTF-8 input and fixed-hash-hook boundaries. Padding remains
in signatures, prefixes are ignored, and only the second colon segment is read.
`encryption-oracle.cjs` executes actual source and installed noble/bs58 libraries
once for eight digest inputs, eight encoding strings and 83 signing cases. It
also captures lone-surrogate behavior and mutation/restoration of noble's global
hash hook. `encryption_controls.py` requires three named production failures and
three passing restorations. All live oracles record Node major/version and arch.

`errors.rs` mirrors expected order-rejection matching and its optional logging
hint. Regex case folding and digits are ASCII, matching the source's non-Unicode
regex literals; patterns stay unanchored. `errors-oracle.cjs` selects complete
actual declarations with the TypeScript AST and executes those declarations,
including their regex array. One Node process returns the whole corpus. The
native logging map is checked directly. `errors_controls.py` mutates case folding
and the native suppression boolean independently and requires named failures
followed by passing restorations. Account-validation classification remains
pending because its dynamic error/status coercion needs a separate witness.

`margin.rs` extracts the two stateless margin conversion field initializers and
uses the shared `MarginType`. Mapping uses Unicode uppercase (dotless i and long
s matter), without trimming. Mode conversion compares the shared enum directly.
`margin-oracle.cjs` executes actual AST-selected arrow initializers and the actual
shared TS enum. It compares each direction independently across case variants,
Unicode/non-string cases and the UTF-8 boundary; `margin_controls.py` mutates
Unicode folding and mode selection with a named failure/restoration per row.

`create_orderly_position_id` delegates its base ID to the shared types helper,
then appends the isolated suffix. The margin oracle also executes the actual
shared TypeScript helper and compares 72 independent symbol/side/mode/margin
combinations, including delimiter-bearing symbols and a surrogate supplement.
`position_id_controls.py` moves the suffix to a prefix and requires the named
source comparison to fail before restoring the production bytes and passing.

`get_current_margin_type` reads the exchange-owned cache first, then the canonical
Store's positions on a miss. It retains missing margin types as distinct entries,
filters strictly positive contracts and defaults ambiguous/absent modes to Cross.
`current-margin-oracle.cjs` executes the original initializer with real lodash
uniqBy. Its Rust witness uses DefaultStore, shared numeric ingress and native
contract bits, including exact signed zero, overflow and a separate typed NaN
supplement. Only NaN bits are canonicalized. Three serial production controls
cover strict positivity, cache precedence and duplicate removal. Production is
read-only; test fixture Store writes explicitly require successful Results.

Divergence audit: all 14 rows at pinned 4ca767911c open with CHOSEN and have
resolved Rust symbols. Six are UTF-8 representation-only, five wire declarations
also validate runtime shape and omit unknown properties, two symbol utilities
omit public memoize-cache injection, and signing omits the mutable global hash
hook. Canonical UTF-8 citations carry that shared rationale; local choices carry
an explicit why-mirroring-would-be-worse sentence. The inherited-table closed
output boundary does NOT justify erased wire-alias validation: those five notes
now name their actual runtime boundary. Unknown-property supplements execute
actual typed source witnesses at every object level and verify Rust omission
separately from the source's retention. No CORRECTED rows or absent Rust targets
were found; no rows were demoted from this audit.

`is_algo_order` compares the three actual algorithmic order enums.
`resolve_margin_type_for_order` preserves cache precedence, closing-side selection,
strictly positive positions and duplicate-position ambiguity. The live AST-selected
order utility oracle covers six order types and 512 resolver cases through the
canonical Store and PlaceOrderOpts. Four production controls require exit 101 at
named assertions and passing restorations. Review found that hardcoding BTC in
ambiguity errors passed the original 511 resolver cases. Those inputs remain
byte-identical (compact UTF-8 JSON: 389105 bytes, SHA256
0fd702f944249e09a1eab63ec3eae1f6938d9f3718ef7c3357d1ac00451cb6e5);
an appended `ambiguous-non-btc` source case now checks the Unicode/delimiter symbol.
This was an error-identity false green, not a precision swap. The current production
precision census has zero price-step reads, zero amount-step reads and zero
implemented two-axis fields to swap. Future order formatters require fractional,
distinct-step source supplements and direct native assertions per payload field.

Cardinality audit (after e5aa776ad6): zero zip walks. Original wire field census,
whole constant equality and logging-map size assertions rejected five compiling
production controls: empty/subset balance holdings, empty/subset Grafana endpoint
table, and empty logging map. This is coverage of those mutations, not a claim
about pending REST/WS collection paths. SHA512 returns a fixed 64-byte array.
The current-margin native-driven input-bit walk now independently checks decoded
positions against source JSON length and source bit-count against JSON length.

A separate real false green existed in source admission: clearing all 512 resolver
rows or dropping one compiled and passed the original target. Every top-level
source array now has an exact count assertion across all seven targets. Wire
array observations additionally check source lengths against raw input and native
lengths independently. `cardinality_controls.py` checks 15 mutations: five original
production controls, four empty/subset source-row controls (resolver and algo),
two production holdings controls with the old field census bypassed, and four
nested literal empty/subset controls. Literal witnesses pin values and compare
decoded native enum variants before serialization. Each
must compile, panic at its named assertion with exit 101, restore, and exit 0.
The bypass controls fail `ORDERLY-CARDINALITY wire/native array/...`.

No oracle or fixture values changed: seven full captures remained byte-identical
on Node v26.7.0 / arm64. Wire capture: 235882 bytes, SHA256
9438ee1aff1454121700b397c7f155e4a572795e175b08e9bc527e138112317f;
order utility capture: 390210 bytes, SHA256
243534b38adfd6cdaf8bf199f8085d9011f36a5c43877ce70ad15bc5bfdcd969.

Extended audit after integration ba6468e0076f: inspection covers seven parity
targets, zero zip walks, five wire-type inventories, three literal subcorpora,
the variable-length BalanceResponse.holding field, constant tables and the
optional singleton logging map. REST/WS batch implementations remain pending;
this census does not claim mutation coverage for them. Source-driven test loops
now assert executed row counts independently; wire cases additionally assert
fixed field counts by type/mode and exactly three ingress comparisons. Per-type
case/projection/refusal/literal totals prevent one record group masking another.

The control script now contains 35 controls. The extension measured 20 new
compiled failures and passing restorations: three extra-native-output controls,
one extra-holdings control bypassing the field census, fourteen empty/subset
source controls bypassing source-total guards across all seven targets, and two
consuming holdings controls bypassing BOTH native length guards. Holdings fault
injection calls the real deserializer to completion, then clears/pops the decoded
vector; no trailing-element error is involved. With both length guards bypassed,
direct native observation still fails at
`wire/BALANCE_RESPONSE/full/native/holding/ingress0`. These are injected faults,
not evidence of a live production decoder defect. The earlier 15 controls were
measured in the preceding audit; this extension does not relabel those runs as
new-head measurements. Source/oracle files and fixture values remain unchanged.

The enum residual from PR 7264 has a permanent production control in
`wire_controls.py`: swap BUY/SELL renames in both public EditOrderSide serialization
and private EditOrderSideWire deserialization. The symmetric codec swap compiles
and fails `literal/EDIT_ORDER_REQUEST/side/decoded-native` with exit 101; restoring
production bytes passes the scoped wire target. This checks native enum identity
before serialization, independently of the record Observe witness. The appended
control is index 12; the runner requires exit 101 plus the named panic.

`get_position_side_for_order` ports the actual asynchronous initializer with a
once-invoked awaited getter. It returns the source uppercase side or None for
undefined, and preserves the getter's error identity. The position-side oracle
executes the actual initializer and shared enums in one Node process: 12 typed
combinations, two rejection identities and one pending/resumed call. Fixed source
counts and native index walks prevent shortening a capture from shortening the
test. Mode and reduceOnly inversion controls require named exit-101 failures and
passing restorations. The getter's own REST body remains separately rowed.


`native_exchange` constructs all six Orderly family names from real options.
`binding` implements only their runtime/import/HTTP/socket primitives on the
shared Scope/Js layer. All 16 Exchange methods execute existing source bodies;
the four inherited unsupported methods retain the exact `Not implemented`
error. `fetchPositions` retains its source balance write. The mutable source
disposal flag is set before socket disposal; source disposal does not invoke
BaseExchange cleanup. Private subscriptions remain server-auth-gated.
P4 signing-exception recovery uses the shared authenticate_and_subscribe wrapper
around the unchanged private onOpen body. This intentional difference and the
native cleanup boundary are named in BINDING_GATES.md and the PR description.
`tests/exchange_trait.rs` constructs production exchanges over scripted transport;
its all-16 method witness and production-disabling controls are described in
BINDING_GATES.md. The registry's native server test verifies exact Ed25519
preimages for all six names. The native registry shares hosted HTTP signing and account metadata with typed
availability. Source private HTTP still signs locally before the hosted adapter;
private WS never reads remoteSigner. Empty secrets retain the source HTTP error
and the already approved shared WS onOpen recovery. See HOSTED_SIGNER_EVIDENCE.md.

The public `set_all_leverage` trait hook invokes Orderly account leverage, even
when the Store has no positions. The existing member validates finiteness, clamps
and rounds 1..100, writes CROSS leverage once, skips unchanged values and advances
its cache only after success. Binding regressions cover those source behaviors
and emitted, swallowed failures through the actual signed HTTP client.

Public trait cancellation invokes the existing Orderly cancel-all/symbol members.
Both ordinary and algo endpoints are attempted in source order, even with an empty
Store or a failed first request; source errors are emitted and swallowed. The real
binding target checks both trait paths, signed symbol queries and continuation.
Private execution and position frames assert Store changes and actual fill events.
Orderly subscribes to executionreport, algoexecutionreportv2 and position only;
balance comes from the REST fetchPositions path, not a private balance topic.


Public ticker, BBO and mark-price frames enforce the approved strict Ticker row
policy at the native Store boundary. A null/string/undefined numeric field
refuses that complete row; valid siblings update before deferred reports name
each refused symbol, field and value. Later source property or Store errors
retain precedence. This intentionally differs from TS retaining malformed values.
`exchange_trait` mixed-row regressions run each topic through all six real account
classes and sockets. The source-only stream oracle retains its raw-value Store;
it does not substitute for these native typed-boundary witnesses.


The constructed ticker census in `tests/exchange_trait.rs::production_ticker_coercion_matches_constructed_typescript`
uses the maintained TS exchange and DefaultStore via `parity/scripts/ticker-strict-source.cjs`.
120 packed three-row frames run through all six account constructors. Every copied ticker/BBO/mark field has strict row-refusal cases; funding rate/time use actual camel/snake alias parsing.
The corpus includes null, strings, NaN-producing inputs, objects, booleans, missing
fields, raw numeric overflow and signed zero. It compares all eleven native number
fields before JSON; approved decision-3 refusal preserves the entire bad row.
`parity/scripts/ticker-native-controls.py` separately disables mapping and refusal
behavior, requiring a compiled native failure and exact restored pass.

Ticker row admission runs after the real Store lookup. Inherited dictionary targets
without a matching ticker remain source no-ops. Only numeric typed patch errors
from the native Store are deferred; source and subscriber errors still propagate.

The three `ws_controls` structural aliases use exact production Host profiles in
the reviewed generic-alias policy. `STRUCTURAL_ALIAS_REVIEW.md` separates their
native trait classifications from actual six-family host/socket execution and
compiled production controls. The same crate-internal Host re-export is used by
live assembly; do not introduce a second host or an identity fallback.

The four class/HTTP callable aliases are also registered with actual configured
Host profiles and six-family invocation/REST observations in the same review.
`class-alias-controls.py` faults these production capabilities independently.

Ticker admission precedes raw native Store mutation: the material Store deliberately
allows source-shaped fields, so its updateTicker cannot supply typed patch refusals.
Orderly rejects a non-number patch only at an effective ticker target, defers its
report until good siblings finish, and leaves the whole row unchanged. Numeric
patches call the original Store method directly; later source and subscriber
failures retain precedence and prethrow writes. The real exchange_trait cases
check native graph slots before typed projection across all six families.

The private onOpen specific-claims review records bounded disposal/recovery
differences separately from the shared original-cause diagnostic correction.
See `../safe-cex-exchange/PRIVATE_OPEN_REVIEW.md`; no Wasm-only row is closed.
