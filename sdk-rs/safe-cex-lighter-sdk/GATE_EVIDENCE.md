# Current shipped signer scope

The standalone TransactionSigner/SignerError implementation and its exclusive
key-management/hash adapters were removed after a shipped-caller census. The
historical receipts below do not describe a currently assembled native signer.
The real Lighter venue uses TransactionClient with its canonical crypto binding.
See `SIGNER_RETIREMENT_REVIEW.md` for the exact caller/dependency boundary.

# Lighter SDK signing records and key runtime

Fresh branch `feat/rust-lighter-sdk-key-runtime` starts at integration
`d4b45c92c11acf2005262f792d26ac2a56a7cf05`. Implementation checkpoint:
`c51e91cd13`. Integration `775a019fe06014b8b858b3f8f6cbdabac552abaf` merged
cleanly at `439a89c63ba061e019cbd36ecb11a3715cb30b7d`.

**9 rows moved: 7 ported, 2 divergent.** Recursive `tomllib` SDK census:
**238 ported / 99 divergent / 52 pending**, 389 total. Before: 231/97/61.
All nine rows have distinct real Rust targets. These are SDK manifests only.

Existing SDK mappings live in `safe-cex-venue-lighter`, despite the lane handoff
describing different crates. New ports therefore live in the isolated
`safe-cex-lighter-sdk` workspace member. Existing resolved SDK mappings remain
in place. This change edits neither the venue crate nor `exchanges/lighter`
manifests relative to the integrated parent. Incoming venue changes are retained.

## Historical row evidence and buckets

Targets below are qualified within `safe-cex-lighter-sdk/src`.

| Source row | Rust target | Status and evidence |
| --- | --- | --- |
| key-manager.ensureWasmInitialized | key_runtime.rs::ensure_wasm_initialized | Ported at explicit imported Poseidon/scheduler/error-constructor boundaries: eager ready/init; initial-ready return; init rejection retains original cause; post-init readiness error. Four source cases plus synchronous failure and eager execution witnesses. |
| key-manager.isWasmError | key_runtime.rs::key_is_wasm_error | Ported object/non-null/inherited-membership body, over required ECMAScript primitives. Four source cases: own error, null, inherited undefined error, callable with error. Membership does not read the error value. |
| key-manager.generateKeyPair | key_runtime.rs::generate_key_pair | Ported orchestration: initialize first; API receiver; crypto export; local predicate; template error coercion; private then public property reads without added format checks. Four source cases plus imported failure, null result and dropped-awaiter witnesses. |
| key-manager.derivePublicKey | key_runtime.rs::derive_public_key | Ported orchestration: initialize before UTF16/ASCII-hex validation, dispatch exact key, local predicate and error message, PromiseResolve assimilation. Four source cases plus original thrown identity, object identity, thenable and failed-init-before-invalid-key witnesses. |
| signer.TransactionSignature | signing_types.rs::TransactionSignature | Ported value interface with distinct UTF16 hash/signature fields; exact actual-source AST schema and four direct native field witnesses, including lone code units. |
| signer.AuthToken | signing_types.rs::AuthToken | Ported value interface: three independent UTF16 fields, binary64 indices and signed unbounded BigInt. Actual-source AST schema and four direct native field witnesses include negative zero, infinities, NaN and deadlines beyond u256. |
| signer.SignerError | signing_types.rs::SignerError | **FORCED divergent:** native class value preserves name, arbitrary UTF16 message and generic unknown cause. Rust has no JS Error.prototype inheritance, instanceof identity or engine-produced stack. Four actual-source constructor cases and native field/aliased-cause assertions establish the preserved value contract; they do not erase this class difference. |
| signer.SignerError.constructor | signing_types.rs::SignerError::new | **FORCED divergent:** native allocation assigns source name/message and preserves undefined/null/object cause distinctions, pointer identity and later cause mutation. It cannot perform JS Error super construction, install its non-enumerable message descriptor or produce an engine stack. Source descriptor/prototype observations identify the actual missing behavior independently of the class row. |
| signer.isWasmError | key_runtime.rs::signer_is_wasm_error | Ported signer-local implementation of the same object/non-null/inherited-membership source body. Four independently invoked actual-source cases; this is a distinct target, not a second row pointing at the key-manager predicate. |

The two error buckets are justified here and in their own distinct manifest
notes. No precedent from another venue is needed. Generic cause values permit
explicit null and retained object identity; native fields do not claim JavaScript
reflection behavior. The two interface rows concern declared value fields rather
than a JavaScript object allocation/prototype contract.

## Actual-source and native oracle

`tests/key_runtime_oracle.cjs` extracts the unchanged maintained TypeScript
function/class declarations with the TypeScript AST, transpiles those bodies,
and invokes them with controlled imported crypto capabilities. The source
interfaces receive exact AST field/type guards. This is not a reconstructed JS
implementation of any of the nine new bodies.

The independent floor is **20 runtime cases** (four each for five function
rows), **7 extra runtime cases**, **8 interface value witnesses** (four each for
two interfaces), and **4 error constructor cases** shared by class/constructor
evidence. This is a finite witness, not exhaustive ECMAScript coverage. Native
DTO fields, binary64 bits/NaN, BigInt, original cause pointers and live aliasing
are asserted before wire encoding. Runtime output encoding retains raw UTF16
units and follows direct object/exception identity assertions.

`KeyHost` supplies only imports: the ported Poseidon API, JS typeof/null/in/get/
ToString, the already-mapped KeyManagerError constructor, PromiseResolve and
owned scheduling. Its ToString contract is template substitution, including
throwing on symbols, not the more permissive String constructor. Function bodies,
branches, property-read order and error wrapping remain in Rust. A compliant
scheduler retains and advances tasks independently of returned awaiters, with
source continuation order; dropping an awaiter does not cancel the SDK body.
The native handle does not itself implement engine unhandled-rejection reporting
or a complete cross-runtime Promise object/microtask interpreter.

`PoseidonKeyHost` is the production adapter to the already-ported
`PoseidonIndex`, `WasmLoader` and `PoseidonCrypto` export dispatch. Its typed
CryptoValue ABI excludes arbitrary JS objects/thenables; the generic SDK body
retains those through its required primitive capabilities. The bridge test drives
the actual ported index/loader/crypto bodies with controlled imported WASM I/O
and export functions, asserting export receiver, exact key, native outputs and
error conversion. It does not execute WASM or establish curve/CSPRNG correctness.
Existing crypto rows are dependencies, not freshly gated behavioral claims.

The real parity scope builds the actual `safe-cex-parity` binary, indexes this
entire production crate, and selects the nine mapped SDK rows. The 26 exact
exemptions cover imported host/associated-type/scheduler/value-carrier plumbing
and native interface field builders. No pending SDK function body is exempted.

## Gate receipts

Gate commands use `/opt/homebrew/opt/rustup/bin/cargo`, `--jobs 1 --locked`, and
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Yarn uses `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.
Python TOML receipts use `/opt/homebrew/bin/python3` (system Python lacks tomllib).
Generated logs live under ignored `sdk-rs/target`, not in this commit.

Measured at `439a89c63ba061e019cbd36ecb11a3715cb30b7d`:

| Gate | Exit and result |
| --- | --- |
| `cargo check -p safe-cex-lighter-sdk --all-targets --jobs 1 --locked` | 0 |
| `cargo test -p safe-cex-lighter-sdk --test key_runtime --test poseidon_bridge --jobs 1 --locked` | 0; 5 source/native tests plus 1 production bridge test |
| `parity:extract` | 0; Updated 0 manifests, 0 added/orphaned |
| `parity:extract:check` | 0; Would update 0 manifests, 0 added/orphaned |
| Real `tests/parity_scope.py` | 0; 9 rows, 35 public Rust items |

Eight distinct production mutation families, each compiled successfully, failed
with **101 at a named assertion**, then restored byte-for-byte and passed with
**0**. The readiness control ran at `439a89c63b`; the remaining seven ran after
the control-script fix `b3ab78d600e5b3103ecf13cff10d8f29b84a3dc9`. A scoped Git
diff confirms all production/native-test/oracle/index-scope inputs are unchanged
between those heads; only the mutation script changed.

| Family | Named assertion | Broken → restored |
| --- | --- | --- |
| Readiness | `SDK-KEY native output ensure 1` | 101 → 0 |
| Key predicate | `SDK-KEY native output generate 1` | 101 → 0 |
| Signer predicate | `SDK-KEY native output predicate-signer 0` | 101 → 0 |
| Generation | `SDK-KEY native output generate 0` | 101 → 0 |
| Derivation | `SDK-KEY native output derive 0` | 101 → 0 |
| Signature record | `SDK-KEY native signature hash` | 101 → 0 |
| Auth token record | `SDK-KEY native auth accountIndex` | 101 → 0 |
| Error class/constructor | `SDK-KEY native signer error name` | 101 → 0 |

The original key-predicate mutation was rejected by `-D warnings` for unreachable
code before running. That attempt is excluded from the eight compiled controls.
The corrected predicate mutations perform the imported membership check and
force its result false; both reach their named behavioral assertions. No compiler
failure, survivor or invalid control is counted as a negative proof.

## Closing integration and input preservation

Integration `039d308cd99cfd09e22205fb1083c3df5a306548` merged at
`5ae58b139e317af679d9694c3adb17587247a9c7`. The only conflict was
`parity/exempt.toml`; reconciliation preserves the complete incoming set plus all
26 SDK entries. Parsed pair-set equality and unique `rs` targets were asserted.

The closing receipt compares `b3ab78d600` with that merge: **463 tracked inputs
are byte-identical**, including production sources, all tests/control scripts,
SDK Cargo manifest, crypto/types dependencies, parity tool, extractor, TypeScript
SDK/crypto sources, root package scripts and Yarn lock. All **406 transitive
Cargo lock entries** for the SDK and parity tool compare equal; workspace
configuration, exact nine mapped rows and SDK exemptions compare equal. Separate
Git diffs prove the complete workspace Cargo manifest and Cargo.lock bytes are
unchanged, and prove this branch has no venue-crate/venue-manifest edits relative
to the incoming integration head. Restored working-tree production diff is empty.

Node oracle runtime is v26.7.0/arm64/darwin. The TypeScript compiler SHA256 at
initial and closing receipts is
`569177652966bd528c319171c7dd22860dbf72bde116cbc4f644f1d02bb12e39`.
The measured cargo/test/compiled-control receipts therefore remain applicable
after this merge; unchanged native gates are not rerun merely to change a SHA.
The real index and extractor are rerun after reconciliation and restoration.

Closing-head results at `5ae58b139e317af679d9694c3adb17587247a9c7`:

| Gate | Exit and result |
| --- | --- |
| Real SDK parity index after restoring all controls | 0; 9 rows, 35 public Rust items |
| Extractor writer | 0; Updated 0 manifests, 0 added/orphaned |
| Extractor check | 0; Would update 0 manifests, 0 added/orphaned |
| Gate-input preservation receipt and scoped Git diffs | 0; 463 inputs, 406 lock entries unchanged |

The following evidence commit changes this Markdown only; its source/test gate
inputs compare unchanged to the measured heads above.

The push-time refresh additionally merged integration
`076f1ae3492a5c5acf63b637fea6da5cd008cb5f` at
`4586493d519ced3ec6262459e705c5c40c12b4a5`, without conflict. The broad input
receipt correctly rejected a full package.json byte-identity claim: its only
change is the unrelated `venue-oracle:kucoin` test command. Inspected diff confirms
that change; both parity command strings and all other root package configuration
and dependencies compare unchanged. No SDK, crypto, type, tool, oracle or compiler
input changed. The narrowed receipt exits **0**, with **462 byte-identical tracked
files**, unchanged parity command strings, **406 unchanged lock entries**, identical
TypeScript compiler hash, and unchanged SDK row/exemption projections. Separate
workspace Cargo manifest/lock and restored production diffs again exit **0**.
The measured positive/negative gates above remain applicable. This last receipt
does not claim byte identity for the unrelated KuCoin command.

## Limits

No full workspace/crate suite, clippy, wasm build/runtime, live crypto, exchange
network, credential/account test, browser check, deployment or CI result is claimed.
Other 52 pending SDK rows and existing mapped SDK/crypto behaviors are outside
this slice's new behavioral floor. No shared core type/trait was changed.

# Transaction and amount encoding

Branch `feat/rust-lighter-sdk-transaction-encoding` starts at integration
`6c27a13bdd9dc5645060283a5d74459405b7a0e2`. Implementation checkpoint
`67af71c733` ports **8 rows, all ported**, each with a distinct real Rust target
and distinct own note. Recursive SDK TOML census: **246 ported / 99 divergent /
44 pending**, 389 total; before this slice: 238/99/52. No divergent row was
reclassified. Existing signer class/lifecycle and API pending rows remain pending.

Integration `a07e971bb08f867925ff18ca8b13d2f288d0deba` merged cleanly at
`c13a7c64638a32e57f0c30586c57d1e238a13323`. All new code stays in the isolated
SDK crate and SDK manifests; there are no venue-crate/venue-manifest edits.

## Eight row contracts

Targets below are qualified within `safe-cex-lighter-sdk/src`.

| Source row | Rust target | Preserved source behavior |
| --- | --- | --- |
| hashCreateOrderTx | transaction_encoding.rs::hash_create_order_tx | Ordered chain/type/account/API/nonce/expiry prefix, UTF16 identifier padding, signed price/quantity, order metadata and flags, optional stop price including present empty string. |
| hashCancelOrderTx | transaction_encoding.rs::hash_cancel_order_tx | Common prefix, independent market/order padding and ordered concatenation; lowercase-only 0x stripping, empty and multiple-field identifiers. |
| hashTransferTx | transaction_encoding.rs::hash_transfer_tx | Common prefix, token then recipient strings, then signed amount; source BigInt parsing/range failures in order. |
| hashWithdrawTx | transaction_encoding.rs::hash_withdraw_tx | Prefix/token/amount, truthy optional L1 recipient, then optional fast-withdraw flag; absent/empty/false distinctions. |
| hashAuthToken | transaction_encoding.rs::hash_auth_token | Account/API/unsigned64 deadline/chain sequence, full binary64 numeric input and arbitrary BigInt deadline, exact range/error priority. |
| bigIntToHex | amount_encoding.rs::big_int_to_hex | Signed arbitrary BigInt radix16 rendering, optional raw bytes*2 padding target, final 0x; negative signs retain their source position within padded strings. |
| formatAmount | amount_encoding.rs::format_amount | Negative/zero precision guard, Number-to-BigInt conversion, power-of-ten division/remainder, zero-fraction .0, signed remainder padding and trailing-zero removal. Negative amounts preserve the source's unusual formatting. |
| parseAmount | amount_encoding.rs::parse_amount | Exact ASCII decimal grammar over UTF16, split, fractional length comparison before padding, concatenation and BigInt conversion. Fractional/NaN precision is retained rather than narrowed to an integer. |

The five hash-named functions encode the input to Poseidon2. The maintained
source performs no crypto digest operation in these bodies. Each native field is
16 UTF16 units; identifier padding guarantees an integral number of such fields,
so direct native concatenation preserves the imported concatenateFieldElements
check on this ordinary value domain. No hex/address validation is invented.

Transaction views are ordinary typed record values: numeric fields retain f64,
strings retain Vec<u16>, booleans retain their optional source distinctions.
Getter/proxy interception and reflective intrinsic overrides are outside this
value adapter. Private little-endian helpers implement the already-ported SDK
field conversion semantics locally without taking a dependency on the venue
crate. Successful BigInt string conversion uses the shared numeric primitive;
invalid surrogate strings retain their raw UTF16 error message. No source type
row is moved merely because a native input view exists.

The three amount functions are ported **at mandatory language-intrinsic
boundaries**: `TextIntrinsics` provides only BigInt exponentiation and String
padStart/padEnd. Those bindings must implement source ToLength, existing-string
preservation, sign-inclusive padding and engine allocation/errors; no pending SDK
function body is delegated. The SDK body performs its own guards, conversions,
division/remainder, regex decision, split, trimming, concatenation and return.
This slice ships no complete JS interpreter or production allocation-intrinsic
adapter. Tests supply finite compliant primitive bindings. There is no new
allocation budget, integer-width restriction or resource policy in the SDK body,
and no standalone engine/resource-exhaustion parity claim. Native computational
errors preserve name and UTF16 message, with no JS Error prototype/stack claim.

Ten exact new exemptions cover six ordinary-record input views, one error carrier
and three intrinsic trait declarations/methods. They hide no pending SDK body.
The real tool indexes the entire isolated production crate: **17 mapped rows,
53 public Rust items**, including the previous nine rows. Its structural gate
is not a fresh behavioral proof of other mapped SDK or venue rows.

## Source oracle and direct native floor

`tests/transaction_encoding_oracle.cjs` extracts and transpiles all eight
unchanged maintained declarations plus the four actual field-element helper
bodies they import. Ordinary JS intrinsics execute in the VM. The oracle does
not reconstruct any new SDK function body. Runtime: Node v26.7.0/arm64/darwin.

The fixed floor is **80 actual-source cases**: eight distinct inputs for each
of five transaction encoders, twelve each for hex/formatting and sixteen for
parsing. Floors and native result/error assertions are per function, not a single
JSON snapshot. Rust UTF16 output vectors, BigInt output values, error names and
raw error-message units are compared directly before serialization. Numeric
inputs are carried by binary64 bits/NaN; no JSON null substitution narrows them.

Cases include unequal numeric/header fields, negative zero, fractions and NaN,
uint32 and signed/unsigned64 limits, simultaneous invalid values for first-error
priority, lowercase/uppercase prefixes, empty and multi-field identifiers,
lone surrogates in successful identifier data and failed BigInt input, optional
stop/L1-recipient/flag cases, negative and large BigInts, negative formatting,
non-finite/fractional precision, invalid amount grammar and overprecision.
A third native test checks exact raw intrinsic inputs (signed units, untruncated
fractional target, BigInt exponent) and preserves imported failure units.

## Encoding gate receipts

Gates are scoped to the SDK crate with `--jobs 1 --locked`. Native checks and
compiled controls are measured at the merged checkpoint above. Generated logs
remain ignored under `sdk-rs/target`; only this manual evidence is tracked.

| Gate at c13a7c64638a32e57f0c30586c57d1e238a13323 | Exit and result |
| --- | --- |
| SDK cargo check, all targets, jobs 1, locked | 0 |
| SDK tests: transaction_encoding, key_runtime, poseidon_bridge | 0; 9 tests (3 new encoding tests, 6 existing SDK tests) |
| Real SDK parity index | 0; 17 rows, 53 public Rust items |
| Extractor writer | 0; Updated 0 manifests, 0 added/orphaned |
| Extractor check | 0; Would update 0 manifests, 0 added/orphaned |

One compiled production mutation per new function family, all at this merged
checkpoint, each **101 at the named assertion → restored 0**:

| Family | Discriminating mutation | Named assertion |
| --- | --- | --- |
| Create order | Encode quantity in the price slot | SDK-ENC native hashCreateOrderTx 0 |
| Cancel order | Encode market ID in the order ID slot | SDK-ENC native hashCancelOrderTx 0 |
| Transfer | Encode token address in the recipient slot | SDK-ENC native hashTransferTx 0 |
| Withdraw | Invert the fast-withdraw flag | SDK-ENC native hashWithdrawTx 0 |
| Auth token | Swap the account and API fields | SDK-ENC native hashAuthToken 0 |
| BigInt hex | Multiply padding width by 3 instead of 2 | SDK-ENC native bigIntToHex 1 |
| Format amount | Divide instead of taking fractional remainder | SDK-ENC native formatAmount 0 |
| Parse amount | Pad the fractional part at the start | SDK-ENC native parseAmount 1 |

All eight broken receipts include successful Rust compilation, running one
filtered test and the named native assertion. Every restoration passed one
filtered test. No compiler failure or survivor is counted, and working-tree
production bytes are restored. Generated logs are not committed.

The baseline preservation scope includes **649 tracked inputs** and **437
transitive Cargo lock entries** for the SDK/index tool. It includes new and
existing SDK source/tests, numeric/types/crypto dependencies, all SDK manifests,
parity tool/extractor, TS SDK/crypto and the Yarn lock. Relevant parity command
strings and other root package configuration/dependencies are compared separately
so an unrelated venue command change cannot masquerade as a native input change.
TypeScript compiler hash:
`569177652966bd528c319171c7dd22860dbf72bde116cbc4f644f1d02bb12e39`.

No full workspace/crate suite, clippy, WASM build/runtime, live crypto or exchange
network/credential test, browser run, deployment or CI result is claimed. Engine
resource exhaustion and allocation-intrinsic integration remain outside the
finite source/native floor. No shared core source/type/trait changed in this slice.

## Encoding closing merge and preservation

Integration `7d73334d51aab93a006d732997ec203cec61ffbc` merged cleanly at
`84cf4d241e9d0725e5cf569e78de9af8c65d0f86`. The input receipt from the measured
`c13a7c6463` checkpoint exits **0**: all **649 tracked inputs byte-identical**,
all **437 transitive lock entries unchanged**, relevant package configuration/
parity command strings unchanged, same TypeScript compiler hash, identical SDK
row and exemption metadata. Separate complete workspace Cargo manifest/lock and
restored production Git diffs exit **0**; venue crate/manifests compare unchanged
relative to the incoming integration. These receipts preserve the measured
cargo/native-test/compiled-control gates rather than claim fresh executions of
unchanged native inputs.

After restoration and the closing merge, the real SDK index was rerun: **0**, 17
rows/53 public items. Fresh extractor writer/check both exit **0**, zero manifests
updated and zero additions/orphans. Final recursive census: **246/99/44**; all
eight new notes and targets are distinct. The evidence-only commit changes no
gate input. Generated receipts and logs remain ignored.

## PR 7750 conflict repair

Integration `970c8a3a41e3ff7e0da31cbf524f7d6b0e064849` merged into the existing
transaction-encoding branch at `1eb0a5beff323c698596eaed5b2633b5e10f6e43`.
The sole conflict was `sdk-rs/parity/exempt.toml`. Resolution preserves every
incoming `(rs, reason)` pair plus all ten encoding entries; parsed pair-set
union and unique Rust targets were asserted. SDK exemption block order changes,
so an ordered-list identity check is intentionally not claimed. Sorted SDK
exemption pairs compare unchanged. No source, test or SDK manifest changed.

Freshly rerun at that merged SHA:

| Gate | Exit and result |
| --- | --- |
| Scoped SDK cargo check, all targets, jobs 1, locked | 0 |
| SDK tests: transaction_encoding, key_runtime, poseidon_bridge | 0; 9 tests |
| Real scoped SDK parity index | 0; 17 rows, 53 public Rust items |
| Gate-input receipt with explicit exemption set comparison | 0; 649 tracked inputs and 437 lock entries unchanged |

No encoding rows were redone or reclassified. Earlier compiled controls remain
applicable to the byte-identical source/test inputs. This evidence-only follow-up
changes no native gate input; generated logs remain ignored.

# HTTP declarations and referral values

Branch `feat/rust-lighter-sdk-http-types` starts at integration
`61d416782d630e0058f93a515e81fafc9be87187`, after PR 7750 landed. Implementation:
`f9b6b04315`. Integration `372972fbfaa126366d6309f4ba15fa84d07bc7a5` merged at
`0538d76a2073da7719b894427265f12bc73e356f`. The declaration configuration test
was isolated in `3071682f0c0f920929f92768c6f7d575aae75fb0`; that is the measured
native gate and compiled-control checkpoint, containing the integration merge.

**8 rows moved: 6 ported / 2 divergent.** Recursive SDK TOML census is
**252 ported / 101 divergent / 36 pending**, 389 total; before: 246/99/44.
Eight distinct notes and real distinct Rust targets; no older row reclassified.

## Each row's contract and bucket

Targets are qualified within `safe-cex-lighter-sdk/src`.

| Row | Target | Native contract and evidence |
| --- | --- | --- |
| RequestInterceptor | http_types.rs::RequestInterceptor | Ported callable declaration, generic opaque input/output and original thrown values; retained Rc function, direct/native-Future return. Exact AST ABI plus four conforming callable witnesses. |
| ResponseInterceptor | http_types.rs::ResponseInterceptor | Ported response callable declaration with unrestricted generic values/errors, direct/Future result and retained callback/input identity. Its own AST ABI and four callable witnesses. |
| ErrorInterceptor | http_types.rs::ErrorInterceptor | Ported generic callable declaration; any output stays opaque (including host promise objects), original exceptions retained. Exact AST signature and four callable witnesses. |
| Interceptors | http_types.rs::Interceptors | Ported independently optional callback-array fields, preserving absent/empty distinction, cardinality/order and shared mutable arrays/functions. Four configurations and direct native alias assertions. |
| LighterApiError | http_types.rs::LighterApiError | **FORCED divergent:** native class value retains source metadata but has no JS Error.prototype inheritance, instanceof brand, non-enumerable Error message descriptor or engine stack. Actual constructor/descriptor/inheritance witnesses identify those absent behaviors; native fields/details aliases tested. |
| LighterApiError.constructor | http_types.rs::LighterApiError::new | **FORCED divergent:** native allocation preserves fields/default code/details identity, but cannot perform Error super construction or invoke V8 captureStackTrace. Source calls expose all initialized fields/constructor identity, and throwing/truthy non-callable hooks can reject source construction while native allocation still returns fields. This constructor difference has its own source witnesses. |
| ReferralPoints | referral_types.rs::ReferralPoints | Ported value declaration, full UTF16 strings and binary64 point fields, optional shared rewards and ordered shared token array. Exact recursive AST schema and four direct native records with nested alias assertions. |
| ReferralPointsResponse | referral_types.rs::ReferralPointsResponse | Ported single-field declaration holding the original shared points record; four native response witnesses preserve identity, fields and later point/reward/token mutation. Exact source AST shape. |

The two API-error buckets are justified here and separately in each row's own
note, independent of earlier SDK/venue evidence. The constructor defaults code
only when its effective argument is undefined; empty code is retained. Metadata
strings retain arbitrary UTF16, status retains full f64, and generic details retain
the caller's opaque original payload. No V8 hook/stack/prototype behavior is claimed.

Callable and record rows are value-level declaration mappings. Generic values
and exceptions are not narrowed to serde_json. Rc function references and mutable
arrays preserve sharing. CallbackReturn provides native direct/Future handles;
opaque generic values can also represent host promise/function/other values.
This is not a JS function/Promise reflection bridge or HTTP interceptor execution
pipeline, and it does not port the pending API-client lifecycle/interceptor setup.
Optional fields represent effective values, without property-enumeration/accessor/
proxy semantics. Dense ordinary token arrays retain order, duplicates and aliases.

Ten exact new exemptions cover native callback-result inspection, typed callback
array access, interface field builders and two nested anonymous referral records.
They hide no pending SDK function body. The entire SDK crate is indexed against
**25 mapped rows / 71 public Rust items**, including earlier slices.

## Actual source and native witnesses

`tests/http_types_oracle.cjs` reads the maintained TS declarations and guards
exact callback signatures and recursive interface field/type/optional schemas.
Type aliases/interfaces erase at runtime: their callable/record fixtures are
conforming value witnesses, not reconstructed or newly ported source function
bodies. The error class itself is extracted unchanged, transpiled and constructed.
Controlled Error.captureStackTrace imports observe the actual optional source
branch; they do not stand in for the native class difference.

Floors: **12 callable witnesses** (four for each alias), **4 configurations**,
**4 error-constructor cases** shared by class/constructor evidence, **2 extra
source capture faults**, and **4 point/response pairs** (four for each interface).
Native callback/value/exception pointers, mutable array references, UTF16 vectors,
f64 bits/NaN, optional fields, nested token cardinality/order and live aliasing
are asserted before any wire encoding. No serialized DTO snapshot repairs fields.

The four API-error cases cover default/empty/custom code, absent/present details,
lone units, original/live details aliases, omitted/-0/NaN/infinite status and
absent/present capture hooks. Additional source faults cover original thrown hook
metadata and a truthy non-callable hook. Native construction exposes the named
missing effect rather than claiming those cases match.

Gate commands use explicit SDK test targets, jobs 1 and locked dependencies;
TypeScript resolves through the existing absolute SAFE_CEX_TYPESCRIPT_PATH.
Generated logs remain ignored under sdk-rs/target. This manual evidence is tracked.

| Gate at 3071682f0c0f920929f92768c6f7d575aae75fb0 | Exit and result |
| --- | --- |
| SDK cargo check, all targets, jobs 1, locked | 0 |
| SDK tests: http_types, transaction_encoding, key_runtime, poseidon_bridge | 0; 13 tests (4 new, 9 existing) |
| Real SDK parity index | 0; 25 rows, 71 public Rust items |
| Extractor writer | 0; Updated 0 manifests, 0 additions/orphans |
| Extractor check | 0; Would update 0 manifests, 0 additions/orphans |

Seven compiled mutation families cover eight new rows; the class/constructor
share their native field constructor family. Every mutation compiled, reached
one named native assertion with **101**, restored original production bytes and
passed the exact filtered test with **0**. No compiler-only failure or survivor
is counted. Declaration-only callback controls mutate the native typed-array
value access, not an invented TS callback implementation; other controls mutate
field construction. Each tests the declared value mapping at the boundary.

| Family | Named assertion | Broken → restored |
| --- | --- | --- |
| Request callback | SDK-HTTP callback identity request 0 | 101 → 0 |
| Response callback | SDK-HTTP callback identity response 0 | 101 → 0 |
| Error callback | SDK-HTTP callback identity error 0 | 101 → 0 |
| Configuration | SDK-HTTP config count request 1 | 101 → 0 |
| API error class/constructor | SDK-HTTP native error name | 101 → 0 |
| Points record | SDK-HTTP native number totalPoints | 101 → 0 |
| Response record | SDK-HTTP native number response totalPoints | 101 → 0 |

The baseline preservation scope contains **654 tracked inputs** and **437
transitive Cargo lock entries**, including new/old SDK source/tests, all SDK
manifests, numeric/types/crypto dependencies, indexer/extractor, TS SDK/crypto and
Yarn lock. Relevant parity command strings and other package configuration are
checked separately; SDK exemptions compare by sorted exact target/reason pairs.
Compiler SHA256:
`569177652966bd528c319171c7dd22860dbf72bde116cbc4f644f1d02bb12e39`.

No full workspace/crate suite, clippy, WASM build/runtime, live crypto or exchange
network/credentials, HTTP client adoption, browser, deployment or CI result is
claimed. Reflective JS function/Promise/record behavior is outside the declaration
value mapping. The API-error runtime differences remain divergent in their own
rows. No shared core source/type/trait was changed.

## HTTP declaration closing integration

Integration `f364cee096ff92584c157cad8b96f12b92cf3a09` merged cleanly at
`85b88f9046901dee775827bbcc6a3993d074f7fd`. Preservation receipt from the measured
`3071682f0c` head exits **0**: **654 tracked inputs byte-identical**, **437 lock
entries unchanged**, identical relevant package/parity command configuration,
same TypeScript compiler hash, unchanged SDK manifests and exact sorted exemption
pairs. Separate complete workspace Cargo manifest/lock and restored SDK source/
test diffs exit **0**. Venue crate/manifests compare unchanged to the incoming
integration. Native cargo/test/compiled-control receipts therefore remain
applicable; no fresh execution of unchanged native inputs is implied.

After all controls restored and integration merged, real index: **0**, 25 rows
and 71 public Rust items. Fresh extractor writer/check both exit **0** with zero
manifest updates/additions/orphans. Final census **252/101/36**, and all eight new
notes/targets are distinct. This evidence-only commit changes no gate input;
generated receipts/logs remain ignored.

## API reads and signature wire conversion

Fresh branch base: `f17498f6e1e8b8e8b93c75bc11f2535b2b4bf8eb`.
Implementation `6cf6026ae6` merged integration `198f871b9b` at `5cf375a0e7`;
source-oracle outcome recording was committed at the measured checkpoint
`53eba0bba3e5b64243f888f10e4303e9135f7851`.
This slice moves **8 rows: 0 ported, 8 divergent**. SDK census is
**252 ported / 109 divergent / 28 pending**, 389 total. Every new row has one
real, distinct Rust target and its own distinct bucket note. The Lighter venue
crate and exchange manifest are untouched by this slice. LighterApiClient and
OrderApi class rows remain pending; this slice does not invent complete classes.

All buckets below are justified by this unit's own evidence, independently of
any earlier slice. They are **CHOSEN** representation/runtime choices: Rust could
model the missing realm effects, so language necessity is not claimed.

| Row | Native target in SDK src | Own bucket and observable difference |
| --- | --- | --- |
| ReferralApi | api_reads.rs::ReferralApi | CHOSEN: complete native referral facade has no JS constructor/prototype object, instanceof brand or mutable method descriptors. Actual source class reflection measures those effects; native facade exposes its single implemented query. |
| ReferralApi.constructor | api_reads.rs::ReferralApi::new | CHOSEN: retains exactly one Rc client reference, without JS own-property enumerability/deletability. Source records enumerable/deletable client; native pointer identity and strong count are asserted directly. |
| ReferralApi.getReferralPoints | api_reads.rs::ReferralApi::get_referral_points | CHOSEN: native BigInt decimal formatter bypasses source BigInt.prototype.toString; source override changes accountIndex to patched-bigint. Original nested result identity and conditional auth remain implemented. |
| LighterApiClient.getNextNonce | api_reads.rs::get_next_nonce | CHOSEN: source override changes account_index to patched-bigint; native request retains intrinsic decimal account. Modern numeric/string 200, legacy nonce, raw Get order, coercion/error selection and transport identity remain native bodies. |
| LighterApiClient.getApiKeys | api_reads.rs::get_api_keys | CHOSEN: source override changes account path to /accounts/patched-bigint/api-keys; Rust formats its native BigInt. Repeated axios response.data reads and original returned array are preserved. |
| LighterApiClient.getTransferFeeInfo | api_reads.rs::get_transfer_fee_info | CHOSEN: source override changes transfer-fee URL and destination account parameter; native formatting bypasses that lookup. Fee/min/max conversion order, original percentage bits and UTF16 auth are implemented. |
| OrderApi.getOrderBookOrders | api_reads.rs::get_order_book_orders | CHOSEN: source Number.prototype.toString override changes market_id to patched-number; native binary64 formatting bypasses the realm. Config spreads/precedence, numeric-only 200, original bids/asks, timestamp fallback and errors are implemented. |
| toBase64FromHex | signature_wire.rs::to_base64_from_hex | CHOSEN: source global parseInt override returning 17 produces ERE= for 00ff; native intrinsic parser produces AP8=. Encoder library imports preserve their effects but cannot repair the bypassed parser lookup. |

`api_reads_oracle.cjs` extracts the maintained source methods/helper with the
TypeScript AST and executes their actual bodies, rather than rewriting a JS
reference. There are **48 read cases and 56 signature cases**, plus five source
prototype-override witnesses, one class reflection witness, and one parser
replacement witness. Cases include modern/legacy/error/null envelopes, large
signed account/nonce/fee integers, undefined/empty/lone-surrogate auth, NaN and
signed-zero inputs, overriding request params, raw order arrays, timestamp
fallback, malformed hex, partial/sign/whitespace parseInt behavior, 300 bytes,
btoa priority, Buffer fallback, missing encoders and original encoder failure.
Class constructor/member inventory is taken from the actual source class.

Direct native assertions check client/result reference identity,
constructor ownership, signed-zero market/percentage bits and a large fee.
Ignored return futures still dispatch eagerly and execute the owned continuation;
transport errors retain their original Rc identity. Imported PromiseResolve is
invoked for raw return values. UTF16 strings and binary64/BigInt values are
retained in the controlled native fixture adapter; the JSON observation codec
is only test transport.

ReadHost is an explicit required ABI for HTTP, scheduling and ECMAScript
Get/CopyDataProperties/truthiness/coercion/error/object/PromiseResolve primitives.
Base64Host imports encoder lookup/call effects. They are not implementations of
pending SDK methods. The tests supply controlled primitive adapters; no complete
JS engine, concrete axios transport adapter, live endpoint, JS prototype bridge
or runtime adoption is claimed. Native intrinsic formatting/parsing differences
remain divergent rather than being scoped away to label these rows ported.

| Gate at measured checkpoint 53eba0bba3 | Exit/result |
| --- | --- |
| Scoped SDK cargo check, all targets, jobs 1, locked | 0 |
| Explicit tests: api_reads, http_types, key_runtime, transaction_encoding, poseidon_bridge | 0; 16 tests, including 3 new |
| Real SDK parity index | 0; 33 mapped rows, 112 public Rust items |
| Extractor writer (first integrated checkpoint 5cf375a0e7) | 0; zero updates/additions/orphans |
| Extractor check | 0; zero updates/additions/orphans |

Seven discriminating controls cover eight new rows; referral class/constructor
share the retained-client construction family. Every broken production operation
compiled, ran one exact test and exited **101 at a named assertion**, restored
original bytes, then passed that same exact test with **0**. Compiler failures
and surviving mutations are not counted. The fixture-only observation adapter
is not mutated in place of production.

| Production mutation family | Named assertion | Broken → restored |
| --- | --- | --- |
| Referral class/constructor ownership | SDK-READ constructor ownership | 101 → 0 |
| Referral endpoint | SDK-READ request referral 0 | 101 → 0 |
| Modern nonce projection | SDK-READ result nonce 6 | 101 → 0 |
| API-key projection | SDK-READ result keys 28 | 101 → 0 |
| Transfer min/max assignment | SDK-READ result fee 32 | 101 → 0 |
| Order timestamp fallback | SDK-READ result orders 34 | 101 → 0 |
| Signature byte conversion | SDK-READ signature result 2 | 101 → 0 |

## API reads closing integration

Integration `9c663a908f40c2955e27b1ce58f97fb5c08fb331` merged cleanly at
`6ec0beb03557eb0de046814acbffdd2a6608b802`. Preservation from measured
`53eba0bba3` exits **0**: **659 tracked inputs byte-identical**, **437 transitive
Cargo lock entries unchanged**, identical relevant package/parity commands and
workspace compiler configuration, exact sorted SDK exemption pairs, and compiler
SHA256 `569177652966bd528c319171c7dd22860dbf72bde116cbc4f644f1d02bb12e39`.
The global Cargo lock has an unrelated venue dependency change; the SDK/indexer
closure is unchanged. Restored SDK production/test working-tree diff exits **0**.
The incoming spec addition describes Bitrue authentication; this slice's
contract is unchanged. Native check/test/control exits apply by byte preservation,
not by claiming they were freshly rerun after the closing merge.

Fresh closing real index exits **0**, 33 rows and 112 public items. Generated
oracle output, mutation logs and preservation receipts stay ignored in target.
This tracked evidence is manually authored. No full workspace/crate suite,
clippy, WASM build/runtime, live crypto, live exchange/credentials/network,
HTTP adoption, browser, deployment or CI result was run or claimed.

Fresh closing extractor writer and check both exit **0**, each reporting zero
updates/additions/orphans. Tomllib independently confirms **252/109/28** and
**8 rows moved**, with eight distinct notes and targets. The venue crate and its
manifest diff against incoming integration exit **0**. This evidence-only commit
changes no gate input; preservation is checked again after committing it.

## Signer runtime method bodies

Fresh branch base: `bcba22af177e3c42704feb058ef7539d58ddcd74`.
Implementation: `7b0b5b7d5d`; integration
`4d4b43c2bfc2ef7649b3a24cb658f9003525bec5` merged at measured checkpoint
`ca9f0131016fd68bc452dfd9016469ebc9fb8941`.
This unit moves **9 pending rows to ported**, with nine distinct real Rust
function targets and nine distinct notes. SDK census: **261 ported / 109
divergent / 19 pending**, 389 total. No new divergence or bucket is introduced.
The signer class and createSigner factory remain pending; no class/prototype,
constructor or complete standalone signer assembly is claimed by these methods.
The venue crate and its manifest remain outside this slice.

All nine functions in `src/signing_runtime.rs` implement their actual method
bodies. Receiver values, arguments, results and errors remain generic retained
handles rather than JSON projections or restricted UTF8 strings. Current
receiver-bound Get/Call, initialized Get/Set, truthiness, comparison, remainder,
template coercion, object construction and PromiseResolve are required runtime
primitive imports. Original already-mapped normalization/WASM-initialization,
transaction/auth hash and crypto export bindings are explicit dependencies.
No pending signer algorithm is delegated into a host method. Native default
ensureInitialized/signMessage bindings dispatch to the new native bodies; the
witness also replaces those bindings as the source permits. The helper selector
and 20 host ABI items are individually exempted, without exempting a method body.

| Source method | Native target in signing_runtime.rs | Preserved contract and finite witness floor |
| --- | --- | --- |
| initialize | initialize | Leading raw state Get outside catch; eager original initialization call; immediate wrapping of synchronous invocation failure; awaited success/failure resumes before Set/catch; exact cause and owned continuation. 7 cases. |
| #ensureInitialized | ensure_initialized | Truthiness/readiness short circuit, original Get/readiness exceptions, exact current SignerError effect. 6 cases. |
| signMessage | sign_message | Current guard; normalization before validation; current string methods and UTF16 length; API/hash/sign order, local inherited error membership, original result aliases. 19 cases. |
| signCreateOrder | sign_create_order | Current guard, original create-order hash binding, unchanged tx/key/chain and current signMessage result/assimilation. 7 cases. |
| signCancelOrder | sign_cancel_order | Cancel-order hash dispatch and current guard/signer calls, eager throws and unchanged raw values/return. 7 cases. |
| signTransfer | sign_transfer | Transfer hash dispatch, original tx identity/chain bits, current signMessage including replacements and thenable/rejected result. 7 cases. |
| signWithdraw | sign_withdraw | Withdraw hash dispatch and source call order, without fixing a replaced guard or signer member. 7 cases. |
| createAuthToken | create_auth_token | Original comparisons/template coercion, raw NaN/fractional/-0 indices, original auth hash, awaited signer result/destructuring, exact six-field object. 12 cases. |
| verifySignature | verify_signature | Current guard/string members, hash/verify order, error getter/inherited membership and raw result through PromiseResolve without bool normalization. 12 cases. |

The oracle extracts maintained key-manager declarations and the full actual
TransactionSigner source class with the TypeScript AST and executes its methods.
The key normalization body itself is maintained source. Hash/WASM/crypto imports
are controlled dependency bindings; their underlying algorithms and a fixed
complete composition are not remeasured here. The actual source SignerError
constructor is wrapped with a Proxy/Reflect observer, preserving its body while
making constructor timing/cause effects visible in the compared event trace.

There are **84 independently fixed source/native cases**, with a hard per-family
floor in each native test, plus one ignored-initialization-future native ownership
test: **10 new tests**. Cases cover state Get/Set faults, sync and awaited init
failures, skipped readiness, initialization short circuit, guard replacement,
replaced signMessage with raw object/thenable/reject, string-prototype replacement,
empty/invalid/full-UTF16 messages and invalid keys, inherited undefined WASM error,
original error getter/API/hash/sign/verify throws, callable/null/raw-object crypto
returns, false verification, NaN/fractional/signed-zero auth indices and a
2^200 deadline. All cases compare eager effects, final effects, result/error and
receiver state.

Direct native assertions precede observation encoding for retained receiver,
tx/API/hash/signature/override-result handles, original exception and cause Rc
identity, numeric bits/NaN and unbounded BigInt deadline. Thus equal encoded
objects cannot disguise a copied or swapped native handle. Dropping the returned
initialize future still runs the source continuation and sets initialized. Raw
return values pass through required PromiseResolve; no reflective native class
or concrete JS promise runtime is substituted for that dependency.

These rows are ported because the bodies preserve the source operations through
required primitive/dependency ABIs, including observable replaced receiver/string
methods; they do not bypass those effects with intrinsic native formatting as
the previous read slice did. This is not a conditional port over a narrowed
ordinary-record profile. The witness adapter exercises finite controlled values;
complete JS engine, getter/proxy/symbol/coercion/GC completeness and a fixed
SDK/crypto/WASM assembly are not implementation claims of that adapter. Import
implementations must satisfy the stated original ABI. Existing class/error or
other typed-boundary divergences retain their independent classifications.

| Gate at ca9f0131016fd68bc452dfd9016469ebc9fb8941 | Exit/result |
| --- | --- |
| SDK cargo check, all targets, jobs 1, locked | 0 |
| Explicit SDK tests: signing_runtime, api_reads, http_types, key_runtime, transaction_encoding, poseidon_bridge | 0; 26 tests, including 10 new |
| Real SDK parity index | 0; 42 mapped rows, 142 public Rust items |
| Extractor writer | 0; zero updates/additions/orphans |
| Extractor check | 0; zero updates/additions/orphans |

Nine production mutations cover the nine new method rows individually. Each
compiled, ran one exact scoped test and failed with **101 at its named assertion**;
original production bytes were restored, then that same test exited **0**.
Compiler-only errors and surviving mutants are not counted. No fixture import
was mutated instead of a production body.

| Family | Named assertion | Broken → restored |
| --- | --- | --- |
| initialize state Set | SDK-SIGN state initialize 0 | 101 → 0 |
| initialization guard | SDK-SIGN output ensureInitialized 0 | 101 → 0 |
| message hash/signature fields | SDK-SIGN native hash signMessage 0 | 101 → 0 |
| create-order hash selection | SDK-SIGN calls signCreateOrder 0 | 101 → 0 |
| cancel-order hash selection | SDK-SIGN calls signCancelOrder 0 | 101 → 0 |
| transfer hash selection | SDK-SIGN calls signTransfer 0 | 101 → 0 |
| withdraw hash selection | SDK-SIGN calls signWithdraw 0 | 101 → 0 |
| auth token/hash fields | SDK-SIGN auth native token createAuthToken 0 | 101 → 0 |
| verification error branch | SDK-SIGN output verifySignature 2 | 101 → 0 |

## Signer runtime closing integration

Integration `faee40c0372da23d6558ad3da511c44a22b05331` merged at
`493dca1102e69122554894afccd503721e39aed9`. The shared exemption conflict
was reconciled by preserving the exact incoming target/reason pairs and all
21 own signing ABI additions; no incoming entry was removed or replaced.
Preservation from the measured checkpoint exits **0**: **663 tracked inputs
byte-identical**, **437 transitive lock entries unchanged**, identical relevant
package/parity commands and workspace compiler configuration, unchanged mapped
SDK rows and sorted exact SDK exemption pairs. Complete workspace Cargo manifest
and lock diffs also exit **0**, as do restored SDK production/test working-tree
and venue crate/manifest diffs against incoming integration. TypeScript compiler
SHA256: `569177652966bd528c319171c7dd22860dbf72bde116cbc4f644f1d02bb12e39`.
The native check/test/control receipts remain applicable by byte preservation;
no new execution of unchanged native inputs after closing integration is implied.

Fresh closing real index exits **0**: 42 rows, 142 public Rust items. Oracle
JSON, mutation logs and preservation receipts remain ignored under target;
this tracked gate sheet is manually authored. No full workspace/crate suite,
clippy, WASM build/runtime, real Poseidon/Schnorr digest/signature execution,
live exchange/credentials/network, concrete HTTP or JS runtime adoption,
browser, deployment or CI result was run or claimed.

Fresh closing extractor writer/check both exit **0**, with zero manifest
updates/additions/orphans. Tomllib independently confirms **261/109/19** and
**9 rows moved**, all nine ported, with distinct targets/notes. This evidence-only
commit changes no gate input; preservation is repeated at its committed head.

## Client runtime implementation and push order

This unit implements the nine remaining api-client rows in client_runtime.rs:
class facade, constructor, four interceptor callbacks, error conversion,
integrator/order payloads, every serialization branch and single/batch submission.
Eight body rows are ported at mandatory raw receiver/runtime and original helper
ABIs. The class row is independently **CHOSEN divergent**: Rust retains a native
host/frame and provisions all 19 member bindings, but lacks a JS constructor/
prototype/instanceof identity and reflective method descriptors. That choice is
not forced by Rust and does not rely on the earlier read slice's bucket.
The 12 existing member and safe-number/base64 dependency ABIs must preserve their
original source behavior; this unit does not silently compose their earlier
restricted implementations or change those rows' classifications.

Per the corrected workflow, the implementation is merged and pushed before
scoped gates. Source/native cases, discriminating controls and named fresh exits
will be recorded after that implementation push. No gate success is claimed here.

## Shipped Lighter API error constructor follow-up

The historical DTO-only error rows above do not describe the shipped venue
constructor after its runtime repair. The production binding now delegates to
the installed Error constructor/prototype and executes the optional capture hook.
Non-enumerable message, inherited toString, field order, raw UTF16 diagnostics,
callback receiver/constructor/details identity and abrupt capture outcomes have
actual NativeRuntime/production-Realm regressions. The metadata DTO remains a
field carrier; it does not itself run JS construction. Shared engine stack
generation remains OPEN, without fabricated traces. See
`../safe-cex-venue-lighter/ERROR_CONSTRUCTOR_REVIEW.md`.
