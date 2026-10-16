# Bitget gate evidence

Types/protocol slice, tested in `feat/rust-venue-bitget` from `6922766a6c`.
No real credentials, signatures against a live venue, or live orders are used.

- Live TypeScript oracle covers all 15 exported protocol constants and all 9
  declared wire types, including nested unknown fields, nullable/optional axes,
  and ECMAScript number formatting. Frozen and live Rust tests both pass.
- Independent on-disk Rust mutation: `RECV_WINDOW` 20000 → 20001 fails
  `live_types_and_protocol`, naming `protocol constant RECV_WINDOW`, exit 101.
- Independent on-disk TypeScript mutation: `RECV_WINDOW` 20_000 → 20_001
  fails the same named assertion, exit 101.
- Rust optional-field mutation: the first `present` decoder changed to
  `nullable` fails `FETCH_SINGLE_ACCOUNT_RESPONSE data.assetMode:null`, exit 101.
- TypeScript optional-field mutation: `assetMode?: string` made required fails
  `FETCH_SINGLE_ACCOUNT_RESPONSE data.assetMode:missing`, exit 101.
- Every mutated file was restored in `finally`; the restored live test exits 0.
- CI wiring audit: 50 passed, including removal/masking of the Bitget step and
  pinning the root command. The step is unconditional in the types job and
  consumes its existing isolated TypeScript installation. Existing source
  triggers already include both `sdk-rs/**` and `packages/safe-cex/**`.

- `cargo fmt --check`, strict scoped Clippy, and scoped wasm build pass.
  Negative controls respectively remove formatting (exit 1), add an unused
  private function (exit 101, named), and add a wasm-only compile error
  (exit 101, named). Restored commands pass.
- Parity check/report/progress each reject a mapped `RECV_WINDOW` row marked
  pending (exit 1, naming the row). Restored status/invention/report pass.
- Independent sub-agent review in a separate checkout: MERGE. Reviewer mutated
  Rust `POSITION_SIDE.long` and TS nested `feeDetail[].fee` independently;
  both failed by name at 101 and restored green. Exact JSON at 1e20 confirmed.
- All 41 endpoint keys carry an explicit Classic v2 / Unified v3 generation.
  Reviewer follow-up: MERGE. Rust generation mutation and independent oracle
  metadata mutation each fail at 101 naming `API generation ACCOUNT_SPOT`;
  removing a classification fails `endpoint generation inventory` at 101.

Additional extraction gate results are appended after execution. Counts are
manifest claims, not proof that the unimplemented exchange or sockets work.

- Extractor: adding `BITGET_NEGATIVE_CONTROL` to the actual TS file fails
  coverage at exit 1 naming the symbol; restoring it yields zero changed,
  added, or orphaned rows. No source mutation remains.
- Frozen generator mutation changes `BROKER_ID` in the snapshot and fails its
  named snapshot assertion at exit 1; restored generator passes.
- Final manifest checkpoint: 203 pending / 24 ported / 0 divergent.

## UTF-16 boundary correction

A follow-up adversarial probe found that a lone-surrogate prefix before
`environment is incorrect` reaches the TypeScript predicate, while serde's
JSON decoder rejects it first. The earlier unrestricted wire parity claim
was too strong. Eight arbitrary-string wire shapes now explicitly record the
existing shared UTF-8 boundary divergence. Each has both high- and low-surrogate
live witnesses (16 total), with doubly escaped input JSON so the test can pin
TS acceptance and Rust rejection without making the oracle itself undecodable.
Reviewer follow-up: MERGE after this correction; four frozen/live tests pass
including the pending pure-function slice. Scalar Unicode probes agree.

## Family and API predicate slice

- Live source comparisons: 143 family combinations, 1008 API error combinations,
  22 validation-state cases. All four frozen/live Rust tests pass.
- Independent Rust then TS `__stop_loss` suffix mutations fail by
  `isBitgetVirtualAttachedTpSlOrderId`, exit 101. Independent Rust then TS
  `40085` silent-code mutations fail by `shouldSilenceBitgetResponseError`,
  exit 101. Restored live tests pass.
- Five arbitrary-string predicates explicitly share the UTF-16 limitation;
  source-derived witnesses pin results despite Rust's decoder rejecting inputs.
- Adversarial isolated review: initially DO NOT MERGE on the unrecorded Unicode
  boundary; after classification and witnesses, MERGE with no remaining finding.
- Scoped Clippy, wasm build, ESLint, extractor check, and parity check pass.
  CI audit passes 50/50; the pinned command now includes the pure generator.
- Working checkpoint after this slice: 196 pending / 18 ported / 13 divergent.
  Divergence count acknowledges input representation omissions, not TS fixes.
- Retained the review's scalar Unicode probes in the maintained generator,
  including U+FEFF and U+0085: current totals are 234 family, 1792 API, and 36
  validation cases; all four Rust tests pass after fixture regeneration.

## Order side and sizing slice

- Actual TypeScript class members and actual safe-math execute in the maintained
  oracle: 72 side, 117 rounding, and 832 sizing cases, including warning order,
  errors, NaN, infinities, negative zero and first-position selection. Six
  frozen/live tests pass across the crate.
- Independent on-disk Rust and TypeScript mutations of the Classic v2 close
  side each fail by `getOrderSideV2` at exit 101. Independent mutations of the
  floor comparison each fail by `floorToLot` at exit 101. Each mutation targets
  its owning function, is restored, and the restored live test passes.
- Isolated adversarial sub-agent review: MERGE. Expanded 7488 sizing cases
  pass; changing first-position selection to last-position selection fails
  `selectPlanTypeAndAmount` at exit 101, then restores green.
- Classic v2 reduction side is a position side; Unified v3 retains trade
  direction and carries a separate posSide. The manifest records this distinction.
- Arbitrary IDs and symbols retain the existing shared UTF-16 boundary
  divergence, with double-escaped live witnesses; no silent sanitization.
- Scoped strict Clippy, wasm build, ESLint, extractor and parity check pass.
  CI wiring audit passes 50/50 with the order oracle pinned in the venue script.
- Manifest checkpoint: 186 pending / 26 ported / 15 divergent.

## Account-mode predicate slice

- Maintained oracle executes four current exchange declarations over 860
  account cases. Six crate frozen/live tests pass. Independent Rust and TS
  mutations fail `isBitgetUtaClassicUnsupported` and `getBitgetApiErrorMessage`
  at exit 101; restored live test exits 0.
- Isolated adversarial review: MERGE. Whitespace-trimming mutation fails
  `getBitgetApiErrorMessage` at exit 101, restoring frozen/live tests to green.
- Three arbitrary-string helpers declare the existing shared UTF-16 boundary
  and retain double-escaped witnesses; the fixed message constant is ported.
- Scoped Clippy, wasm, ESLint, parity check and extraction check pass. The
  correspondence gate initially rejected an unnamed ApiCode split; its note now
  identifies the exact representation target, and the gate passes.
- Manifest checkpoint: 182 pending / 27 ported / 18 divergent.

## Signing reference preparation (API rows remain pending)

- 23 offline captures execute the actual TS createAPI interceptor, real Axios
  request transforms, qs and HMAC, recording at the hash update boundary and
  the adapter boundary. The transport factory is substituted, so this does not
  establish transport/lifecycle or Rust API parity. No venue network is used.
- Missing optional object fields omit from both views. Dollar stripping and
  underscore rewriting change only the signed body; the latter changes string
  values too. Array/string bodies are spread for signing but sent in their
  original shape. Query encoding differs between preimage and adapter URI.
- Mutation of the Rust-consumed reference snapshot and independent TS underscore
  replacement mutation each fail `symbol_dollars` at exit 101; restored test
  passes. This is capture-sensitivity evidence, not a Rust implementation control.
- Isolated reviewer: MERGE as reference only. Independent TS encode:true
  mutation fails `unicode_whitespace` at exit 101 while adapter URI is unchanged;
  restored capture passes.
- Seven tests pass through the mandatory venue script, generators all pass,
  CI wiring audit 50/50 and ESLint pass. Counts remain 182/27/18
  (pending/ported/divergent); createAPI and toJsonString remain pending.

## Inverse pure helpers

- 72 independent inverse-side cases execute the actual inverse TS method;
  Rust delegates the identical main-variant body. Five deterministic UUID
  streams pin all 31 draws, hex digit boundaries, version and variant bits.
- Rust and TS version-digit mutations each fail `generateUuid` at exit 101;
  independent Rust mode flip and TS side flip fail `getInverseAlgoOrderSide`
  at exit 101. Restored live tests pass.
- Isolated review: MERGE. Independent variant mask 0x3 to 0x7 mutation fails
  both frozen/live tests by `BitgetInverseExchange.generateUuid`, exit 101;
  restored two tests pass.
- Scoped Clippy, wasm, ESLint and extraction pass. Parity correspondence
  required exact target names in receiver-free adaptation notes; notes updated.
- Manifest checkpoint: 180 pending / 29 ported / 18 divergent.

## Literal-union accepted-set audit

- Found a Rust-only widening: BITGET_UTA_CATEGORY accepted the object
  {"USDT-FUTURES":null}. Expanded TS-derived rejection probes fail that case
  at exit 101. All three literal members now receive object/array/case/whitespace
  probes (2014 total wire cases).
- Fixed the public Deserialize operation to require a string before a private
  remote serde decoder. Canonical names are declared once. Initial review was
  DO NOT MERGE because remote=Self exposed a public bypass; moving the generated
  decoder to a private companion fixes it. Qualified-call regressions reject
  tagged objects for every category member. Follow-up isolated review: MERGE.
- Four frozen/live category metadata and type tests pass. Independent private
  serde alias injection and TS literal change each fail the named
  `serde accepted-set inventory BITGET_UTA_CATEGORY` at exit 101, restored green.
- Swept all venue enums: ApiGeneration has Serialize only, so no accepted-input
  surface; ApiCode is an open string/number union, not a closed literal union.
  Seven consumed shared enums have matching serde-derived STRING inventories,
  now pinned in the maintained tests, including rejected strings.
- Separately measured all seven shared enums accepting tagged objects: OrderSide,
  OrderStatus, OrderType, OrderTimeInForce, PositionSide, PositionMode and
  OrderStopTriggerType. Routed to shared owner; this venue did not edit those
  types. Shared string inventories do not claim their object rejection is fixed.
- Counts unchanged: 180 pending / 29 ported / 18 divergent.

## History source reference preparation (mapper rows remain pending)

- Actual TS helpers and mapper bodies execute over 30 numeric, 384 leg, 292
  fill, 305 bill and 3 batch cases. Tagged capture retains undefined, negative
  zero, nonfinite results and function-valued prototype categories; JSON output
  is captured separately. Resolver/timestamp callbacks record invocation order.
- Independent TS commission-sign and category-fallback mutations fail named
  fillCases/billCases at exit 101. Rust-consumed reference snapshot mutation
  fails numbers case 0 at 101. Restored reference test passes. These prove
  capture sensitivity, not Rust mapper equivalence.
- Isolated review: MERGE as reference only. Reviewer replaces actual fills
  parseFloat with Number: numbers case 6 (BOM plus 12.5tail) fails101; restored0.
- Corpus includes cTime numeric1e21, non-callable own toString TypeErrors,
  early guards, callback throws, batch interruption, fee rebates/overflow,
  gross PnL versus commission, and inherited Object.prototype category keys.
- Counts unchanged: 180 pending / 29 ported / 18 divergent.

## Struct accepted-set sweep

- All 17 public wire records already require maps through flattened extra
  fields. Endpoint and UtaOrderSide have no Deserialize implementation.
- Added complete positional sequences in TS field order at every object node
  (2034 maintained wire cases), plus qualified direct Deserialize calls for all
  17 public records, including nested records. Valid objects still pass.
- Removing first flatten in favor of default fails the named complete-sequence
  check for FetchSingleAccountResponseData, exit101. Independently widening
  the TS response declaration to allow string arrays fails by name, exit101.
  Restored frozen/live tests pass.
- Isolated reviewer: MERGE. Independent nested BitgetUtaOrderFeeDetailItem
  flatten-to-default mutation fails both tests by struct name, exit101;
  restored two tests pass. No new production defect found in this sweep.
- Counts unchanged: 180 pending / 29 ported / 18 divergent.

## Typed PnL pure decisions

- 976 maintained cases execute actual TS PnL methods and their actual finite
  helper. Rust operates on the declared numeric Position/Ticker fields, with
  no unknown/string parser substitution. Covers finite guards, absolute
  contracts, API/overflow fallback, accumulation and lookup ordering.
- Independent Rust and TS arithmetic mutations fail calculateUpnlFromLastPrice
  at101; account guard mutations fail calculateAccountUpnl at101. Restoredgreen.
- Isolated reviewer: MERGE. Moving ticker lookup after the zero-contract guard
  fails both tests by calculateUpnlFromLastPrice lookup order, exit101; restored
  two tests pass. Lone-surrogate symbol witnesses retain shared UTF-8 divergence.
- Scoped Clippy, wasm, ESLint, extraction and parity check pass.
- Checkpoint: 178 pending / 29 ported / 20 divergent.

## Shared coercion merge

- Merged utilities 5cf6b4d8d8 into 38b77f2c2d; resolved the CI audit,
  exemptions and formatter documentation per hunk. Shared numeric spelling
  delegates to the canonical implementation rather than retaining a copy.
- Named workflow steps: 55 on each parent, 56 after merge; no missing
  step from either parent. CI wiring audit: 52 passed.
- All 10 Bitget oracle tests and 7 shared coercion/number tests pass.
  Scoped venue Clippy and wasm checks pass; parity invention/status pass.
- Isolated sub-agent merge review: MERGE, including both-parent step union.
- Counts unchanged: 178 pending / 29 ported / 20 divergent.

## Classic fill coercion and leg helpers

- num, totalFeeOf and legAndDirectionOf use shared fallible String(any) and
  parseFloat. No local numeric conversion shim. Live/frozen oracle checks
  30 numeric (one explicit non-JSON NaN limitation), 17 fee and 384 leg cases,
  plus three double-escaped UTF-16 boundary witnesses.
- Rust and TS num mutations independently fail named num cases at101; both
  languages' one-way suffix mutations fail legAndDirectionOf cases at101.
  Restored live test passes. Independent reviewer changes fee addition to
  assignment: both tests fail totalFeeOf case13 at101, restored two pass.
- Isolated review: MERGE. All12 venue tests, scoped Clippy and parity/extraction
  pass. Rows explicitly record JSON runtime-domain and shared UTF-8 limits.
- Checkpoint: 175 pending / 29 ported / 23 divergent.

## Account and position numeric selection

- toFiniteNumber and firstFiniteNumber use the shared trim/parseFloat operations;
  unknown inputs project by source typeof without String(any) coercion.
  Maintained actual-class oracle:174 scalar and3365 selection cases, including
  NaN/infinity/signed zero fallbacks, BOM/NEL, numeric prefixes and hostile objects.
- Independent Rust and TS parseFloat-to-Number controls fail toFiniteNumber
  by name at101. First-to-last iteration controls fail firstFiniteNumber in
  each language at101. All restored green.
- Isolated reviewer:MERGE. Replacing NaN sentinel with caller fallback fails
  both tests at firstFiniteNumber case33,101; restored2 pass.
- Full venue12 tests, scoped Clippy/wasm and parity/extraction pass.
- Checkpoint:173 pending /29 ported /25 divergent.

## Account/position mode, margin and side normalization

- 2015 actual-class cases cover fallible String conversion, direct-property
  null/undefined TypeErrors, field precedence, empty versus nullish values,
  case conversion without trimming, BOM/NEL and Unicode. UTF-16 witnesses
  separately pin shared String/Value rejection; JSON-domain limits documented.
- All three functions have independent Rust and TS literal mutations failing
  named normalization cases at101, restoredgreen.
- Isolated reviewer:MERGE. Treating empty posMode as missing fails both tests
  at normalization mode case139,101; restored2 pass.
- Full venue12 tests, scoped Clippy/wasm and parity/extraction pass.
- Checkpoint:170 pending /29 ported /28 divergent.

## Success parsing and liquidation fallback; shared Store finding

- 28 success-code and448 liquidation cases execute actual source bodies with
  shared radix10 parseInt and parseFloat in Rust. First matching position,
  TypeError propagation, signed-zero, NaN and infinity branches pinned.
- Rust/TS radix mutations each fail isBitgetSuccessCode at101; first-to-last
  position mutations each fail resolveLiquidationPrice at101, restoredgreen.
  Independent reviewer normalizes stored infinity: case12 fails101.
- FINDING: actual TS Store and real DefaultStore over six prices prove typed
  insertion retains nonfinite; both a nonfinite JSON patch and an unrelated
  contracts patch of existing nonfinite position panic in update_in_array.
  TS retains the values without error. This is a shared defect, not parity;
  native catch_unwind only pins the finding and is not a wasm workaround.
- Reviewer TS sanitization control fails named preservation assertion at101;
  Rust panic-suppression control fails named unrelated-patch FINDING at101.
  Restoredgreen, isolated review MERGE. Shared owner notified.
- Full venue12 tests, scoped Clippy/wasm and parity/extraction pass.
- Checkpoint:168 pending /29 ported /30 divergent.

## Family API profile getters

- Actual source getters run with real inheritance for USDT/USDC/inverse and
  absent/false/true testnet flags. All six product/margin values are preserved.
  TS overrides falsely annotate USDT literals; apiUtaCategory also falsely
  casts S-prefixed testnet categories to a union that rejects them. Both
  source defects are marked and retained in the actual runtime outputs.
- Rust/TS USDC-product and testnet-category-correction controls each fail
  named profile cases at101, restoredgreen. Reviewer margin mutation also
  fails by name101, restoredgreen; isolated verdict MERGE.
- Maintained extra live test compares uppercase over all1,112,064 Unicode
  scalars; passes Node26.7 and CI-family Node22.23.2, both Unicode17. Independent
  Rust/TS lowercasing controls fail named scalar assertion101, restoredgreen.
  Separate UTF-16 witnesses retain the shared string-boundary limitation.
- Twelve-test venue suite plus new scalar test pass; scoped Clippy/wasm and
  parity/extraction pass. All later cargo capped jobs2 and serialized with
  reviewer. Extra scalar test independently reviewed and executed:MERGE.
- Checkpoint:161 pending /35 ported /31 divergent.

## Lost-as-const oracle audit

- Swept all six venue oracle scripts. None builds a static expected TS type
  from tuples, assertions or index types: tables and functions execute actual
  source and compare concrete values; wire accepted sets inspect declarations
  directly, with unsupported tuple/index syntax failing closed.
- Existing on-disk Rust and TS controls cover each oracle family, including
  source Store sanitization versus Rust panic suppression. Wrong Rust values
  fail named runtime assertions; TS type-annotation widening cannot make these
  runtime equality checks vacuous. No lost-as-const defect found.
- Counts unchanged:161 pending /35 ported /31 divergent.

## Market resolution and inverse margin coin

- 840 actual-source cases per helper pin lookup traces, duplicate precedence,
  first array match, exact suffix order, terminators and BOM/NEL. The source
  USD-before-BUSD defect is retained; silently correcting it fails the oracle.
- 284 inherited-property witnesses document CORRECTED typed-Market behavior,
  including a real Rust fallback Market where TS returns an inherited function.
  Explicit own constructor entries work. Fixed witness count detects a source
  correction rather than silently dropping the divergence branch.
- Rust/TS first-to-last, BUSD-order and inverse USD-suffix mutations all fail
  by helper name at101, restoredgreen. Reviewer underscore-removal mutation
  fails lookup order case20 at101; restored3 pass. Isolated review MERGE,
  including strengthened fallback/witness inventory.
- Full13 venue tests pass; scoped Clippy (nested-if style corrected), wasm and
  parity/extraction pass, serialized jobs2.
- Checkpoint:159 pending /35 ported /33 divergent.

## Divergence taxonomy audit

- Read the repository MANIFEST-SPEC.md at integration commit29ef23169f via
  git show; this branch has not yet merged that documentation commit.
- All33 divergent rows already named their causes. Made ten multi-cause notes
  explicitly MIXED:23 FORCED-only shared UTF-8 boundaries,10 MIXED rows with
  CHOSEN JSON projection, CORRECTED typed-Market prototype handling, or the
  independent shared Store FINDING retained in full. No status/mapping changes.
- Independent sub-agent review: MERGE; all33 explanations and unchanged counts
  checked. Metadata parity and extraction gates pass; no runtime changes.
- Checkpoint:159 pending /35 ported /33 divergent.

## Symbol normalization

- 1050 actual-source cases cover all six family/testnet products, product-before-
  legacy suffix stripping, case sensitivity, stacked suffixes, BOM/NEL and line
  terminators, fallible String coercion, empty resolver symbols and throw timing.
  Two live UTF-16 witnesses retain the existing shared boundary limitation.
- Separate on-disk Rust and TS mutations for case-sensitive legacy stripping and
  empty-symbol truthiness each fail normalizeBitgetSymbol by name at101.
  Restored live oracle passes. Independent reviewer removed _SUMCBL handling:
  both frozen/live tests fail case65 at101, restored3 pass; review MERGE.
- Full13 venue tests, scoped Clippy and wasm, parity and extraction pass with
  serialized jobs2. Counts:158 pending /35 ported /34 divergent.

## Unified v3 position mapper

- 1290 actual-source cases cover numeric field precedence, fee generation,
  hostile String coercion, nullish liquidation fallback, typed infinities,
  overflow, signed zero, optional margin size and resolver/clock timing.
  Two double-escaped live witnesses preserve the shared UTF-8 limitation.
- Separate Rust/TS Classic-fee substitution and avgPrice-priority reversal
  controls fail mapUtaPosition by name at101; restored live oracle passes.
  Independent reviewer changed margin-size >0 to >=0: both frozen/live tests
  fail mapUtaPosition case417 at101, restored3 pass; review MERGE.
- The mapper retains typed nonfinite values. The existing real Store witness
  still pins the independently routed update_in_array panic; no workaround.
- Counts:157 pending /35 ported /35 divergent.
- Full13 venue tests, scoped Clippy, wasm, parity and extraction all pass,
  serialized with jobs2 after review and restoration.

## Core response-bearing REST ABI merge

- Parents845739ce20 (venue) and ddb86eb3b8 (core, includes ed304ce751).
  Named CI steps:56 /55 ->56, with both parent name sets retained. Workflow
  wiring plus live layer audit:104 passed. Exemptions resolved per hunk.
- Initial compile caught core scaffold cleanup silently removing all Bitget
  dependencies. Restored only used production types/num/exchange/serde/json and
  Store as a dev dependency; unused transport/crypto/candle-cache stay removed.
  Scoped Cargo check regenerated only the Bitget dependency block in the merged
  lockfile. No shared layer-policy edits.
- Initial CI audit invocation lacked Cargo PATH and failed ENOENT; rerun with
  required PATH passes end to end. After dependency repair, full13 venue tests,
  scoped Clippy, wasm, parity and extraction all pass, serialized jobs2.
- Counts remain157 pending /35 ported /35 divergent.
- Independent merge review:MERGE; all56 steps and both parent CI assertions,
  all exemption keys, shared coercions and venue mappings retained. Reviewer
  verified dependency repair against imports and isolated lockfile delta.

## Integration HOW/spec merge

- Parents60a2bc180c /dfa2ff2318: named CI steps56 /52 ->56, both sets retained.
  Canonical sdk-rs/parity/MANIFEST-SPEC.md now lives in this checkout and was read.
- README and tunnel test append conflicts retain both sides per hunk. Incoming
  heartbeat timeout assertion needed the core's existing leaf_error test helper
  for RestFailure; initial compile exposed that old-ABI assertion. No production
  error flattening was introduced. Bitget dependencies and lockfile unchanged.
- Restored gates:66 tunnel tests,4 HOW parser tests (live TS path included),
 104 CI wiring/layer tests,13 venue tests, wasm, parity/extraction all pass,
  serialized jobs2. Counts157 pending /35 ported /35 divergent.
- Independent merge review:MERGE; both README sections, leaf_error and all
  incoming tests retained, REST envelopes coexist with HOW fixes, and venue
  files/dependencies/lockfile/exemptions are unchanged.

## Classic v2 position mapper

- Expanded maintained position corpus to1656 cases per generation. Classic
  invalid-payload fallback, arrays-as-objects, instId/nullish priority, numeric
  field precedence, distinct realized-PnL formula, signed zero/nonfinite output,
  and callback/clock timing compare against the actual class method.
- Separate Rust/TS UTA-fee substitution and arrays-as-invalid controls each fail
  mapPosition by name at101; restored live oracle passes. Reviewer reversed
  instId/symbol priority:case1353 fails at101, restored3 pass; review MERGE.
- Shared undefined info becomes absent in the Rust optional field; explicitly
  CHOSEN own-property distinction despite identical JSON omission. Shared UTF-8
  and Store nonfinite-patch FINDING remain explicit in this row as well.
- Counts:156 pending /35 ported /36 divergent.
- Full13 venue tests, scoped Clippy, wasm, parity and extraction all green,
  serialized jobs2 after review and mutation restoration.

## Shared primitives integration merge

- Parents921ba3867f /7738215fb0: named CI steps56 /55 ->56, no parent name lost.
  Bitget dependencies and shared layer policy remain unchanged; scoped compile
  and104 CI wiring/layer tests pass. Conflict hunks preserve Bitget assertions,
  canonical formatter delegation, core ABI/new primitive exemptions and exactly
  one LoadedChunks entry.
- First venue gate correctly fails old shared-enum metadata collector after
  strict string decoders land. Collector now obtains private derive metadata
  through unknown_variant errors, retaining aliases; all seven shared enums
  additionally reject tagged objects, arrays, null, booleans and numbers.
- On-disk Rust WireOrderSide alias=bid and independent TS Bid='bid' mutations
  both fail named serde accepted-set inventory OrderSide at101; restoredgreen.
- Counts:156 pending /35 ported /36 divergent.
- Independent merge/harness review:MERGE. Reviewer bypassed the shared string
  guard: named enum rejects non-string OrderSide {buy:null} fails at101,
  restored2 types tests pass. Full13 venue tests, Clippy and wasm pass.
- Parity gate remains RED at pinned incoming7738215fb0:math_log10/math_pow
  lack exemptions in that parent itself. Upstreama6f317fbc9 supplies them;
  follow immediately with the newer integration merge. Mutable origin refs
  advanced during work, so comparisons use pinned parent SHAs/MERGE_HEAD.

## Shared primitive registration follow-up

- Pinned parents866c77aff1 /bf9e30f7c1: named CI steps56 /55 ->56.
  Automatic merge retains every exemption, Bitget dependency and implementation.
  Independent read-only reviewer: MERGE; no lost names or duplicate exemptions.
- Scoped crate compile, all13 venue tests, wasm,104 CI wiring/layer tests,
  parity check and extraction check pass. The prior missing math exemptions
  are supplied upstream; no venue workaround was added.
- Counts:156 pending /35 ported /36 divergent.

## CI execution-location audit

- All eight Node-spawning Bitget tests are ignored by default; native Cargo
  discovers but does not execute them. The mandatory TYPES job invokes the
  full venue oracle after frozen root installation. Runtime npm packages are
  available there; compiler selection alone is isolated, not the full runtime.
- No selector edit or hermeticity claim. A new ordering assertion plus a
  deliberate reorder control pins Bitget after root installation.105 CI/layer
  tests pass; independent read-only review confirms MERGE.
- Counts remain156 pending /35 ported /36 divergent.

## Classic per-fill execution mapper

- mapBitgetFill/mapBitgetFills and callback dependencies consume shared
  fallible String, radix10 parseInt, parseFloat and truthiness.326 source cases
  pin direction, gross profit, rebate negation, signed zero, callback evaluation
  order, object feeDetail[0], batch rejection and first-error propagation.
- Nine non-JSON NaN inputs and two lone-surrogate witnesses retain source
  outcomes while explicitly pinning the chosen JSON/shared UTF-8 boundaries.
  Optional ID omission differs from TS own undefined property, noted on rows.
- Independent on-disk Rust and TS rebate-abs substitutions fail named
  mapBitgetFill fee:240 at101; ID truthiness substitutions fail named151/152
  at101. Restored3 scoped tests pass. Maintained child source-correction test
  requires mapper assertion failure; replacing its mutation with a no-op fails
  its own source-correction assertion at101.
- Reviewer:MERGE. Independent object numeric-key removal fails named
  objectFeeDetail:282 at101 and restores green.
- Full14 venue tests, scoped Clippy, wasm, parity and extraction pass jobs2.
  Counts153 pending /35 ported /39 divergent.

## Classic income mapper

- num/mapBitgetBill/mapBitgetBills use shared coercions and truthiness.335
  source bill cases and3 batches preserve gross amount, signed fee, suffixes,
  callback-free error propagation, nullish/falsy distinctions and zero-sum
  non-trade emission. Seven NaN inputs and two surrogate inputs pin boundaries.
- All12 Object.prototype keys captured;48 nonzero cases retain source function
  or object categories and interpolated tranIds beside Rust Other/other. This
  is CORRECTED by the CHOSEN shared closed category enum, never called forced.
- Separate Rust/TS gross-PnL and fee-sign mutations fail named bill case6 at101;
  coin nullish substitution case92 and sum-only guard case243 fail at101. The
  initial Rust coin control only failed on an unused import; removing that
  import in the mutant then produced the required named runtime failure.
- Maintained child income-fee correction detector requires named mapper101;
  a no-op mutation fails its parent assertion101. Restored3 scoped tests pass.
- Reviewer:MERGE; independently removed fee incomeType suffix, failed named
  bill case6 at101, restoredgreen. Counts150 pending /35 ported /42 divergent.
- Venue oracle, scoped Clippy, wasm, parity and extraction all green after restoration.

## Signing replacement-token reference

- Added five actual HMAC-boundary/adapter captures (28 total) for $$, $&,
  prefix and suffix replacement tokens plus a repeated-key value. Prefix and
  suffix expansion can produce invalid signed JSON; adapter data stays intact.
  createAPI/toJsonString remain pending the routed shared replace-first API.
- Rust frozen-expectation mutation and separate on-disk TS literal replacement
  mutation both fail key_dollar_dollar at101. Maintained child controls require
  named literal and all-match failures; each no-op substitution fails its own
  parent assertion at101. Restored2 signing tests pass.
- Initial reviewer replaceAll substitution SURVIVED at0. Added repeated dollar
  key before its property; independent replaceAll now fails the named
  key_repeated_in_value at101. Reviewer:MERGE after repair.
- Full venue gate (15 tests), Clippy, wasm, parity and extraction green.
  Counts unchanged150 pending /35 ported /42 divergent; no API parity claim.

## Pure routing and suppression state getters

- Actual source getters compared over24 declared boolean combinations,
  including missing/false/true optional testnet. Explicit receiver-state
  projection matches the existing profile helper pattern.
- Separate Rust/TS conjunction-to-disjunction controls fail named
  usesUtaRouting case6 at101; suppression inversion controls fail case0 at101.
- Reviewer:MERGE. Independent testnet-negation control fails routing case18
  at101; restored3 order tests green. Full15 venue tests, scoped Clippy, wasm,
  parity and extraction pass. Counts148 pending /37 ported /42 divergent.

## UTA strategy replacement options

-3036 actual source builder cases cover enum combinations, missing/present
  undefined triggers, copied-field subset, signed zero and nonfinite numbers.
  Source own-key/undefined-key inventories pin the shared typed option
  representation choice; two surrogate cases pin the shared string boundary.
- Rust/TS trigger-copy swaps fail named cases2/7 at101. Rust/TS abs(price)
  controls fail named case3000 at101. The first TS abs control hit an earlier
  unrelated price site and survived; targeting the builder body fixed the
  control rather than changing the implementation or counting that result.
- Maintained child source correction detector requires named mapper failure;
  replacing its mutation with a no-op fails the parent assertion at101.
- Reviewer:MERGE. Independent amount-to-remaining substitution fails named
  case0 at101; restored4 order tests pass. Full16 venue tests, Clippy, wasm,
  parity and extraction green. Counts147 pending /37 ported /43 divergent.

## Canonical shared-string choice and local rationale audit

- Overseer ruling supersedes earlier boundary-relative FORCED wording. Rust
  can express UTF-16; retaining the shared UTF-8 contract is CHOSEN. Added one
  canonical declaration in parity/MANIFEST-SPEC.md with the alternative, why
  the shared representation remains, and an inventory/update obligation.
- All43 divergent rows cite the stable shared-utf8-string-boundary anchor.
 24 have no other divergence;19 keep their local CHOSEN/CORRECTED/FINDING
  explanations and an explicit why-retain rationale. No FORCED labels remain
  in Bitget manifest notes. Counts and Rust mappings are unchanged.
- Independent read-only review:MERGE; confirms source-bug markers, correction
  witnesses and Store findings retained. Parity/extraction gates green.
  Counts147 pending /37 ported /43 divergent. No production change or new tests.

## Integration 0716 and canonical-anchor reconciliation

- Resolved CI append conflicts by union: parent named steps56/62, merged63,
  no lost names from either parent. Dependencies remain intact. The single
  declaration uses the published b8de291 section and retains all43 existing
  shared-utf8-string-boundary citations; shared-repr:utf8-string is superseded.
- Crate check, wasm check, full16 Bitget tests (including live oracles),115 CI
  wiring tests, layer audit, parity check and extraction all pass. Initial layer
  invocation lacked cargo PATH and failed ENOENT; corrected invocation passed.
- Counts147 pending /37 ported /43 divergent, unchanged.
- Reviewer found the numbered heading generates a different Markdown fragment.
  Added explicit shared-utf8-string-boundary HTML anchor; canonical section text
  remains byte-identical and the43 citations now resolve.

## Open mapOrder outputs and table lookup inventory

-81 actual-source mapOrder cases retain own undefined, numeric/object IDs,
 all12 inherited names at5 fields plus strategy side.26 protocol expressions and
 86 dynamic-map expressions are inventoried by AST, with source-key expressions
 maintained. Initial uppercase-only17 count was incomplete and is superseded.
- On-disk Rust expectation and independently TypeScript own-property-only lookup
 controls each fail named mapOrder assertions at101. Maintained child correction
 detector passes; replacing its mutation with a no-op fails at101.
- Reviewer same-named function substitution initially survived: name-only tagging
 was insufficient. Native members now match by reference, with unmatched function
 identities separately tagged; repeated mutation fails named constructor case101.
 Undefined ingress probe now removes source own-undefined fields as JSON.stringify
 does, instead of substituting null. Baseline shared Order still accepts.
- Income inherited-category correction reclassified as unresolved FINDING pending
 shared TableHit/open ledger consumers. Two statuses remain divergent; mapOrder
 remains pending. Counts147 pending /37 ported /43 divergent.
- Re-review MERGE; reviewer repeated native identity control at101 and restored
 live test green. Final17 venue tests, Clippy, parity and extraction pass.

## Pure private-stream normalization

-31 normalization cases,133 nullish alias cases,13 terminal-fill membership
 cases and174 finite-number cases execute actual private class members. Two
 double-escaped surrogate witnesses and3 non-finite JSON-domain exclusions retain
 source outputs. Shared fallible String/parseFloat and existing numeric adapter
 are reused; no local JS coercion or socket owner is introduced.
- Ten on-disk controls, each Rust and independently TypeScript: replacing all
 separators with first-only, retaining underscores, treating empty status as
 missing, admitting success as a terminal fill, and swapping the finite branch.
 Each fails its named helper assertion at101. Restored6 order tests pass. The
 maintained child source detector passes, and its no-op control fails at101.
- Full18 venue tests, Clippy, wasm and parity pass. Counts142 pending /37 ported
 /48 divergent, awaiting independent review.
- Reviewer precedence swap initially survived: single populated aliases did not
 distinguish state from planStatus. Added4096 simultaneous alias combinations
 (including null/empty/falsy and throwing losing aliases), bringing raw cases
 to4229. Rust and separate TS priority swaps now fail named getRawOrderStatus
 at101; restored6 order tests pass. Extraction reports zero drift.
- Re-review MERGE: independent priority swap now fails getRawOrderStatus case285
 at101; restored scoped test passes.

## Integration TableHit and lodash merge6eb262afc5

- Clean per-file merge preserves both parent inventories:63/66 named CI steps
 become67, no missing names. Crate dependencies remain intact. Canonical UTF-8
 declaration has exactly one explicit anchor and no obsolete identifier.
- Crate check, layer audit,134 CI wiring tests, full18 venue tests and parity
 pass. Counts142 pending /37 ported /48 divergent. TableHit now exposes the
 inherited-member return needed by getCancelPlanType; shared open Order/Income
 consumer boundaries remain separate findings.

## Cancellation plan selection through shared TableHit

-3264 actual getCancelPlanType cases span9 plan mappings, unknown/empty/falsy
 values, JSON String coercions/errors, all12 inherited members, order variants,
 rounding, NaN/infinity/signed zero, warnings and unknown-type errors. Two
 surrogate witnesses retain source warning strings. Shared TableHit returns
 native-member identity instead of taking the fallback or widening an enum.
- Rust inherited-to-missing mutation fails named101. First TS own-only control
 failed the AST inventory instead of behavior; retaining both lookups repaired
 the control and it now fails getCancelPlanType by name101. Rust/TS mapping-value
 and rounded-versus-raw sizing controls also fail named101. Maintained child
 own-only detector passes; no-op mutation fails101. Restored7 order tests pass.
- Shared TableHit identity is not callable JS code or a mutable prototype object;
 that representation and typed Order input are explicitly CHOSEN on the row.
 Counts141 pending /37 ported /49 divergent, awaiting review.
- Independent review MERGE: moving trailing-stop fallback before raw lookup
 fails getCancelPlanType case560 at101, restored scoped test passes. Full19
 venue tests, Clippy, wasm and parity green.

## Private-stream position reconciliation fold

-336 actual-source timestamp cases and706 record-selection scenarios cover
 nullish alias stopping, signed zero, invalid finite parsing, inclusive5000ms
 freshness, future/non-finite clocks, fresh snapshot filtering and stable input
 reference identity. Duplicate IDs retain last-current replacement but first
 unused-current append; duplicate REST inputs are not silently deduplicated.
 Map/Set keys include prototype names, BOM/NEL, astral text and NUL. Two source
 surrogate witnesses and3 non-finite metadata projection witnesses retain the
 shared input-domain boundary without claiming invalid JSON.
- Rust and independent TS mutations of alias order, duplicate winner, snapshot
 boundary and zero-time truthiness each fail named101. Maintained child REST
 deduplication correction detector passes, and no-op control fails101.
- Counts139 pending /37 ported /51 divergent, awaiting final gates/review.
- Full20 venue tests, Clippy, wasm, parity and extraction pass. Review MERGE:
 independently filtering appended current positions by snapshot IDs fails case436
 at101; restored scoped test passes.

## Integration merge d8bcccd413

- Retargeted the uncommitted590f merge after upstream published the canonical
 lock repair and actual ProtoMember::js_string. The earlier resolution passed
 all20 venue tests,176 CI checks, compile/layers/parity/extraction and independent
 read-only review; it was not committed as a separate shared lock repair.
- Final parent named CI steps67/71 become72, zero missing. Exemptions259/273
 become273, zero missing, duplicates or blank reasons. Resolved per hunk;
 relocated core exemptions are retained exactly once. Bitget dependencies intact.
- Canonical incoming lock now supplies candle-cache Redis/Tokio/browser packages;
 scoped cargo check --locked --offline passes without regenerating it.
- Corrected section15 is present with its explicit anchor: whole-record refusal
 never equals field omission, and __proto__ serializes as an object. All51
 divergent rows cite UTF8:26 are UTF8-only and25 have additional axes; no row yet
 cites closed-output. Attribution does not discharge the live income Other
 fallback or the declared market prototype branch differences.
- Verified ProtoMember::js_string and replacement wrappers in the actual merged
 tree rather than relying on the earlier landing announcement. Both pure-helper
 dependencies are now available; Store nonfinite patch findings remain open.
- Final merged tree:20 venue tests,181 CI wiring tests, locked offline crate
 compilation, layer audit, parity and extraction all pass. No manifest drift.
- Independent final delta review MERGE: both-parent preservation, canonical
 lock plus only Bitget dependency entry, shared primitive/spec identity verified.

## Income inherited output refinement

- Shared TableHit preserves inherited non-nullish lookup and member-specific
 ProtoMember::js_string now preserves tran_id for all12 names. Missing keys
 still map Other. Existing optional category projects inherited values to None
 while retaining the record; no wider enum or second ledger type is introduced.
- Twelve added source witnesses assert typeof, exact native identity and own
 category before serialization. Eleven callable fields disappear in JSON;
 __proto__ remains{}, separately pinned as a FINDING against Rust omission.
 Parsed numeric values are compared without claiming byte/key-order equality.
 The first identity witness incorrectly compared the __proto__ accessor on
 Object.prototype itself (which yields null); repaired it to compare the actual
 table result with Object.prototype. Frozen/live source gates now pass.
- Existing335 bill cases and48 inherited outputs retain the source's member
 interpolation without rewriting expected tran_id to the old other suffix.
 Counts remain139 pending /37 ported /51 divergent;26 UTF8-only,25 with added
 axes, of which2 now also cite the closed typed output boundary.
- Four on-disk controls independently restore Other, collapse member strings
 to constructor, correct the TS lookup to own-only, and substitute the raw
 type for interpolated category. Each compiles/executes and fails a named
 assertion at101. Maintained own-only child control passes; replacing its
 mutation with a no-op fails101. Restored frozen/live helper tests pass.
- Full20 venue tests, host Clippy, wasm compilation, parity and extraction
 pass. Separate wasm-target strict Clippy (--all-targets) exits101 in unchanged
 shared candle-cache indexed_db.rs on two collapsible_if diagnostics, before
 linting Bitget; this is not a wasm Clippy pass and is routed upstream.
- Independent adversarial review MERGE: retained records/lookup branches and
 member-specific IDs verified; optional category projection and separate
 prototype-object FINDING accurately stated. Upstream wasm Clippy remains red.

## Pure signing serializer

- toJsonString now uses shared JsJsonValue/stringify, own-key ordering and
 first/all replacement operations. Prepared values retain undefined own keys,
 nonfinite numbers, negative zero and BigInt errors; duplicate assignment
 entries enumerate each key once without changing its first insertion position.
-182 direct actual-source cases cover scalar/array/object inputs, undefined
 underscore keys, all dollar tokens, repeated tokens in values before keys,
 and64 paired-key order cases. Every object also runs with duplicate Rust
 assignment entries. Two double-escaped surrogate witnesses retain valid JSON
 source acceptance alongside shared UTF8 ingress refusal.
- The source's blanket serialized text bug is retained, including nested keys
 and string values. This maps only toJsonString, not createAPI; request factory
 and signing/wire orchestration still have pending rows.
- Provisional counts138 pending /37 ported /52 divergent: the new row retains
 the shared prepared-data input domain (no callable hooks, cycles, functions,
 symbols or custom prototypes) with an explicit why-retain rationale.
- Nine independent on-disk controls (Rust/TS first-versus-all, literal dollar
 replacement, reversed key order and erased undefined keys, plus Rust duplicate
 enumeration) all compile/execute then fail toJsonString by name at101. The
 maintained replace-all detector passes and its no-op control fails101.
 Restored direct live comparison passes.
- Full23 venue tests, host strict Clippy, wasm compile and own-crate wasm
 strict Clippy (--all-targets --no-deps) pass. The latter explicitly excludes
 dependency lint; full wasm lint remains blocked on the recorded upstream
 candle-cache diagnostics until the separately announced fix is merged.
- Parity and extraction pass with zero drift. Independent review MERGE:
 first/global order, replacement expansion, undefined keys and duplicate-key
 enumeration preserved; direct string assertions retain signing-byte sensitivity.

## Integration wasm-lint repair 9222ed2454

- Clean merge still audited against both parents:72/73 named CI steps become74,
 no losses. TOML parsed,280 exemptions unique with zero blank reasons and no
 missing entries from273/280 parents. Crate dependencies retained; locked
 offline compilation and layer audit pass.
- Full dependency-inclusive wasm strict Clippy (--all-targets, no --no-deps)
 now passes. This is distinct from the preceding own-crate-only measurement;
 shared candle-cache repair releases the actual target gate.
- All23 venue tests and189 CI wiring tests pass on the merged tree.
- Parity/extraction pass; independent merge review MERGE verifies both-parent
 gate/exemption preservation, dependencies and distinct full wasm lint claim.

## Classic/UTA mapOrder

- 1291 actual-class source cases retain the two generations' alias precedence,
 plan/hedge overwrite, strategy recovery, attached-leg ordering and clock timing.
 Shared TableHit preserves inherited control flow before final shared Order
 projection; no local prototype table or enum widening. Numeric outputs travel
 as tags, so NaN, infinities and negative zero cannot become false JSON matches.
- All12 inherited identities remain visible before serialization. Actual source
 JSON separately proves eleven callable statuses omitted and __proto__ status
 retained as {}. Whole-result Rust refusal is explicitly different from either.
- Two valid-JSON lone-surrogate witnesses and scalar Unicode/BOM/NEL positives
 pin the UTF8 boundary. A live source identity witness mutates gateway then info,
 compares both views, and separately records a later NaN assignment; Rust-owned
 snapshots do not observe either mutation direction.
- Coercions measured at mapOrder: parseFloat applies to amount/filled aliases,
 price and plan/attached triggers, accepting BOM and trailing garbage, rejecting
 NEL as whitespace, retaining Infinity and -0, and parsing hex text as zero.
 Empty strings normally lose the || selection, but [] wins it then stringifies
 empty. reduceOnly interpolation happens after the clock, and owning a
 non-callable toString throws there. Attached ID interpolation can throw after
 the parent was constructed, so projection waits until all legs are evaluated.
- Nine on-disk controls: Rust/TS remaining arithmetic, zero-trigger fallback,
 late hedge overwrite and strategy-leg ordering, plus TS shallow-copying info.
 Each compiled/executed and failed its named mapOrder case at101; restored live
 comparison passed. Two assertions were strengthened to name unexpected success
 instead of producing an unlabelled unwrap_err panic. The maintained own-only
 prototype detector passes; its no-op control fails101 by name. The first no-op
 command used a wrong test filter and ran zero tests; it was discarded and rerun
 against the actual named test, verifying one executed test and exit101.
- Provisional counts137 pending /37 ported /53 divergent. Row attribution keeps
 closed output refusal, raw ID/unknown enum restrictions, JSON input domain,
 owned aliasing and the distinct info numeric FINDING separate. The canonical
 alias declaration is verified at935fc8f578 and will enter with the next merge.
- Final corpus sweep removed shadowed alias probes, enabled the plan branch for
 trigger coercions, and added attached-ID throws, lookup-key coercion, conflicting
 price/symbol aliases and non-USDT product suffixes. The1291-case live comparison
 passes. A separate source-at-call NaN-info witness proves the numeric loss even
 without any later mutation, so the alias declaration cannot absorb that FINDING.
- Full23 venue tests, host strict Clippy, dependency-inclusive wasm strict Clippy,
 parity and extraction pass. Parity first rejected a private inherent projection
 impl; using a private free function resolves scanner ownership without changing
 source behavior. Adversarial review initially HOLD: changing recovered-strategy slTriggerBy/tpTriggerBy market tests to MARKET left the entire source corpus byte-identical. Added147 independently varied one-leg/two-leg recovery cases. Rust and each TS selector now fail named recovery-trigger assertions at101; restored full23 venue tests, full wasm Clippy, parity and extraction pass. Independent review closure MERGE: each isolated TS SL/TP selector mutation changes28 named recovery-trigger cases; no further blocking evaluation-order or attribution findings.

## Integration numeric Store ABI 91db928805

- Parent CI steps74/76 become77, no missing names. Real tomllib parses301
 exemption BLOCKS and301 DISTINCT targets, zero blank reasons, and no missing
 target from either parent. The single wiring-test conflict preserves complete
 Bitget and WOO assertions. Own Cargo.toml dependencies remain intact and locked
 offline crate compilation passes. All three canonical anchors now resolve.
- The former nonfinite unrelated-position panic assertion actually fails101 by
 its FINDING name on this base: false versus the formerly expected true. The
 shared target JSON round-trip is gone. The replacement witness requires typed
 insertion, unrelated contracts patches and native ModelPatch.set_number patches
 to retain all six source prices, including NaN, both infinities and signed zero.
 Lossy legacy JSON null patches return an explicit error without erasing state.
- Rust native-patch sanitization and an independent on-disk TS Store sanitizer
 each fail their named preservation assertion at101, then restore. No shared
 production file remains modified. Three obsolete panic FINDING clauses are
 removed; their other CHOSEN axes remain, so counts stay137/37/53. The direct
 ticker path remains separately scoped.
- Full23 venue tests, host strict Clippy and full dependency-inclusive wasm
 strict Clippy pass separately;196 CI wiring tests, layers, parity and extraction
 pass. Adversarial merged-state review MERGE verifies both-parent script/CI preservation,301 unique complete exemption blocks, retained dependencies/lock, and the justified Store witness flip. Current origin will be rechecked
 before readiness, per the sequential merge-up protocol.

## Incremental integration b1ef25ef36

- Re-fetched integration before readiness; KuCoin and GRVT arrived meanwhile.
 CI step multisets77/78 become79 with no missing occurrence. Both parents' root
 scripts survive exactly (85 total), including Bitget, WOO, KuCoin and GRVT.
 The single conflict retains Bitget's root-install timing assertion and all
 incoming KuCoin/primitive controls. No assertion was weakened.
- Real TOML parse301 BLOCKS /301 DISTINCT, no blank reason or missing target.
 Bitget dependency manifest unchanged; locked offline compilation passes.
 204 wiring tests, layers, parity (6984 rows/1643 public items) and extractor
 (zero drift) pass. Shared production delta is only ws_auth module declaration
 relocation; ci-deps adds axios-retry. Full venue and host/wasm Clippy results
 above belong to reviewed merge91; this additive re-merge uses the coordinator's
 incremental gate rule rather than claiming a redundant full test run.
- Independent current-merged-state review MERGE verifies CI/scripts, exemptions,
 lock/dependencies and unchanged Bitget implementation/manifest. Counts137/37/53.

## Integration get-key merge 6bcff79504

- CI step multisets79/78 become79, no missing occurrence. Both parents' root
 scripts survive exactly. Real tomllib parse:303 exemption BLOCKS /303 DISTINCT,
 zero blank reasons and zero losses from either parent. Own Cargo.toml unchanged.
- Full scoped gates on this merged tree pass: locked offline compilation,23
 venue tests, host strict Clippy and dependency-inclusive wasm strict Clippy
 separately,204 CI wiring tests, layer audit, parity (6984 rows/1649 public
 items), and extractor zero added/orphaned/changed manifests. Logs are
 /tmp/bitget-mergekey-{compile,venue,host-clippy,wasm-clippy,ci,layers,parity,extract}.log.
- No Value-first ingress repair was introduced. The five existing numeric
 requestTime wire fields remain on their prior serde path; no claim is made
 that rebuffering through Value preserves signed zero or solves raw overflow.
 The amended shared ingress requirement is raw-overflow AND signed-zero
 preservation. The pending createAPI note now names remaining factory work
 rather than already-landed stringify/replacement primitives. Counts137/37/53.
- Independent final merged-state review:MERGE. Both-parent CI assertions,
 scripts, dependency selectors and lock entries retained;303 complete unique
 nonempty exemption blocks, correct layer containers, incoming get-key unchanged.
 No ingress-repair or signed-zero claim introduced.

## Integration 8cbfb8b5f9 and numeric ingress audit

- Both-parent CI multisets79/80 become81. The sole wiring conflict preserves
 all Bitget and incoming Binance assertions. Real tomllib:303 BLOCKS /303
 DISTINCT, no blanks or lost targets; incoming js_number_string exemption
 reason replaces older wording deliberately. Bitget dependencies remain intact.
- Before the additional ingress audit, full23 venue tests, locked offline
 compile, host strict Clippy, full dependency-inclusive wasm strict Clippy,
 213 CI tests, layers, parity and zero extractor drift all pass on this merge.
 Logs:/tmp/bitget-merge8cb-*.log.
- Urgent existing-decoder audit finds all five requestTime f64 fields preserve
 raw -0/-0.0, parsed-Value lexical -0/-0.0 and native Value(-0.0), by bits.
 Raw +/-1e400 rejects whereas Node and parsed-Value decoding retain signed
 infinity. Five existing divergent rows gain explicit FINDING clauses; counts
 remain137 pending/37 ported/53 divergent. No Value-first repair is applied.
- The maintained source-derived numericIngress corpus inventories all five
 fields and25 cases with Node f64 bit strings, before JSON erases the sign or
 infinity. Frozen and live tests compare bits, pin raw overflow refusal until
 shared ingress lands, and exercise native -0 per field. An on-disk Rust
 decoder normalization and independently a TS capture normalization each fail
 named parsed numeric ingress FETCH_UTA_ACCOUNT_ASSETS_RESPONSE.requestTime -0
 at101; restored live test passes. Logs:/tmp/bitget-ingress-*-control.log.
- Independent review:MERGE for consolidation and ingress audit separately;
 recursive TS inventory independently confirms exactly five numeric fields,
25 source bit witnesses agree, no production workaround or status changes.
 Post-audit host and full wasm strict Clippy also pass separately.

## Integration b8fc4dfbaa consolidation

- Both-parent named CI-step multisets81/82 become83 with no loss. Three
 wiring conflicts retain complete Bitget, Lighter and Asterdex assertions.
 Both parents' package scripts remain byte-identical. Real TOML parse gives
312 exemption BLOCKS /312 DISTINCT, no missing target, blank or changed reason.
 Own Cargo.toml dependencies remain intact; locked offline compilation passes.
- Full23 venue tests, host strict Clippy and full dependency-inclusive wasm
 strict Clippy separately pass.221 wiring tests, layer audit, parity
 (7334 rows/2275 public items) and zero extractor drift pass. Logs:
 /tmp/bitget-mergeb8-{compile,venue,host-clippy,wasm-clippy,ci,layers,parity,extract}.log.
- Expanded ingress-path audit adds borrowed-Value decoding to every one of
 the25 source bit cases; frozen/live type tests pass after this assertion.
 All five fields retain signed zero and infinity via owned AND borrowed Value;
 raw overflow refusal remains the separately named FINDING. Counts137/37/53.
- Independent current-merged-state review:MERGE; independently221/221 CI
 assertions pass, both-parent metadata and borrowed path verified. Post-borrowed
 assertion host and full wasm strict Clippy pass separately. Incoming Asterdex
 oracle trailing whitespace is unchanged from its parent, outside this slice.

## Incremental integration 00a1a0069e

- Fetched again before readiness. The incoming delta contains only nine
 Lighter implementation/test/manifest files, each retained identically. No
 Bitget or shared production, CI, scripts, lock, dependency or layer changes.
- Independent merged-state review:MERGE. Both-parent CI multisets83/82 remain83.
 Real TOML parse312 BLOCKS /312 DISTINCT, no blank, missing or changed reason;
 root scripts retained exactly. The full scoped b8 gates remain applicable;
 this incremental merge uses the coordinator's explicitly authorized rule.
- Incremental wiring rerun:221 passed, zero failed/skipped; log
 /tmp/bitget-merge00-ci.log. Counts137 pending/37 ported/53 divergent.

## Incremental integration 557c44efba

- Incoming P6 worker consolidation changes no Bitget or shared-dependency
 production code. Unioned the package.json conflict to retain both venue-oracle:
 bitget and worker-oracle. CI83/84 becomes85, no missing occurrence; both
 parents' scripts remain exact. Real TOML312 BLOCKS /312 DISTINCT with no
 missing or changed reason. Own Cargo.toml unchanged.
- Locked offline Bitget compile,227 CI tests, layer audit (38 crates), parity
 (7334 rows/2411 public items), and extractor zero drift pass. Logs:
 /tmp/bitget-merge557-{compile,ci,layers,parity,extract}.log.
 Full venue and separate host/wasm Clippy results remain the b8 scoped run;
 the worker addition leaves Bitget's dependency code unchanged and uses the
 authorized incremental merge checks.
 Counts137 pending/37 ported/53 divergent.
- Independent merged-state review:MERGE; incoming worker workspace/layer
 containers match, all parent gates retained, lock differs only in the retained
 Bitget dependency block, and Bitget implementation/manifests are unchanged.

## Inverse mapOrder slice after resume

- Read RESUME.md before continuation. Current fetched integration200f57e95b
 contains the consolidated Bitget branch; fast-forward changed only the shared
 manifest spec and P6 UI leverage rows/code. The quoted6f419e0992 docs commit
 was not on origin when fetched; its missing crate CLAUDE files were not assumed.
- Important correction to earlier extractor reports: this worktree resolves
 smol-toml from its own node_modules/smol-toml/dist/index.cjs, an extraneous local
 dependency. Local extractor passes do NOT establish clean-install reproducibility.
 The RESUME registry-example and extractor-dependency findings belong to shared
 owners; neither is suppressed or worked around by this venue.
- map_inverse_order executes the inverse source body separately: narrower
 aliases, optional-method reduceOnly, native remaining before table reads, shared
 Math.round conversion, and no linear unknown-type strategy recovery. The
 private projection retains typed output refusals until after source branches.
 Primary/attached info snapshots and separate nonfinite info loss are witnessed.
- Maintained1130 source cases execute the real inverse class member and actual
 Store constructor/update. Missing/false/true inverse flags, negative half ties,
 nonfinite numeric outputs, alias conflicts, throwing values, inherited hits
 paired with a valid raw side, plan/hedge overwrites and excluded linear aliases
 are covered. Callable/prototype identity is captured before JSON serialization;
 two surrogate witnesses retain the UTF8 boundary and scalar Unicode positives.
- Ten independent on-disk controls (Rust and actual TS source each for rounding,
 boolean coercion, zero-trigger fallback, extra quantity alias, and own-only side
 lookup) compile/execute then fail named inverse mapOrder assertions at101.
 Restored live oracle passes. Logs:/tmp/bitget-inverse-control-*.log.
- A maintained child correction detector mutates source boolean reduceOnly.
 Substituting a no-op makes the detector fail its own named assertion at101;
 restored detector passes. Logs:/tmp/bitget-inverse-{maintained*,noop*}.log.
- Full23 venue tests before adding that one detector, host strict Clippy and
 full dependency-inclusive wasm strict Clippy separately, parity, layer audit
 and locally-resolved extractor pass. The additional detector passes separately.
 Final post-review scoped validation is recorded below. Candidate136/37/54.
- Adversarial review found a surviving eager triggerPx evaluation mutation and
 missing extra-object alias coverage. Added a truthy triggerPrice paired with a
 throwing fallback (and its zero-trigger counterpart); reviewer independently
 measured the eager mutation now changing exactly the named short-circuit case.
 Source primary/SL/TP extra identity and both mutation directions are now pinned
 against independent Rust OrderExtra snapshots; the row cites the snapshot
 boundary for this axis separately from info. Reviewer verdict MERGE;9/9 order
 tests, including ignored live controls, pass after repairs.
- Final scoped validation:24/24 venue tests (zero ignored), separate host and
 dependency-inclusive wasm strict Clippy, parity7334/2413, layer audit38 and
 local extractor0 added/0 orphaned all pass. Extractor clean-install caveat above
 remains open. Real tomllib parse:312 BLOCKS/312 DISTINCT, zero blank reasons.
 Logs:/tmp/bitget-inverse-final-{venue,host,wasm,parity,layers,extract}.log.

## Public and secondary-public ticker helpers

- Both actual TS class bodies execute independently:5702 ticker cases,80 scalar
 cases,146 explicit native-NaN JSON-input-domain witnesses and4 UTF16 witnesses.
 Field order, absence and numeric bits are asserted before serialization.
 Nullish selection precedes validation; native output retains percentage overflow
 from finite input, negative zero and the absence of rejected fields.
- Nine on-disk controls: Rust and each TS socket separately, for signed-zero
 normalization, replacing nullish selection with truthiness, and changing percent
 scaling. Every control fails a named assertion at101, then restored live green.
 Logs:/tmp/bitget-ticker-control-*.log and /tmp/bitget-ticker-restored.log.
- Independent reviewer MERGE. Its additional derived-overflow guard source
 mutation changes20 named secondary-socket cases, including both infinity
 witnesses. Scoped2/2 ticker tests including live source passed in review.
- Final26/26 venue tests, separate host strict Clippy and dependency-inclusive
 wasm strict Clippy, parity, layers38 and local extractor zero drift pass.
 Logs:/tmp/bitget-ticker-final-*.log. The documented extractor clean-install
 dependency gap remains; this local pass is not proof of clean-install wiring.
 Real tomllib:312 BLOCKS/312 DISTINCT; measured132 pending/37 ported/58 divergent.

## Fills-page read body

-622 maintained actual-source cases at xhr.get's parsed response.data boundary.
 One call forwards the Classic ORDER_FILLS endpoint and parameter values. Error
 names/messages and original read-rejection identity/metadata are separate
 assertions; the returned list is neither filtered nor reordered. Array identity,
 nested edits and growth in both directions pin the owned-snapshot choice.
 Raw overflow and signed-zero row numbers are tagged before JSON transport.
- Six on-disk controls: Rust and TS independently, success-code comparison,
 cursor truthiness and row order. Each fails a named _fetchFillsPage assertion
 at101 and restored live passes. Logs:/tmp/bitget-page-control-*.log.
- Adversarial review MERGE. Independent source mutation replacing array
 validation with truthiness changes38 named cases. Three scoped tests pass,
 including the live oracle and dropping a pending supplied read.
- Final29/29 venue tests, host strict Clippy, dependency-inclusive wasm strict
 Clippy separately, parity7334/2416, layers38 and local extractor zero drift pass.
 Logs:/tmp/bitget-page-final-*.log. Local extractor caveat remains as above.
 Measured131 pending/37 ported/59 divergent. HTTP assembly/decoding and the outer
 pager remain pending; this slice neither claims nor substitutes those operations.

## Numeric-ingress integration merge

- Actual incoming parent b283acad08 contains c659b17b74 plus two spec-only
 corrections. The shared remote-tracking ref advanced between verification and
 merge; subsequent merges must use the verified SHA rather than a moving ref.
- CI named steps85/87→87, none missing from either parent. Real tomllib parse:
 323 BLOCKS/323 DISTINCT, zero blank reasons or missing parent entries. Bitget
 dependencies and layer containers unchanged. Independent merged-state review
 returned MERGE against the actual b283acad08 parent.
- Scoped compile,29/29 venue tests, host strict Clippy and dependency-inclusive
 wasm strict Clippy separately, parity, layers38 and CI wiring237/237 pass.
 Local extractor zero drift retains the documented extraneous-dependency caveat.
 Logs:/tmp/bitget-mergec659-*.log (run on the actual b283acad08 merged tree).
-131 pending/37 ported/59 divergent. Numeric ingress is available but no Bitget
 decoder has opted in yet; the five requestTime findings remain open.

## Direct requestTime numeric ingress

- Five production derived Deserialize fields now use the shared scalar
 JsNumberInput::decode::<f64> operation. This changes bare raw/owned/borrowed
 record decoding itself, not only a new wrapper API. Value extras stay on their
 original path; no whole-record JsNumberOutput claim or upstream-buffer repair.
-90 maintained source numeric cases compare bits before serialization: signed
 zero variants, signed overflow/underflow, subnormals, finite extremes and
 precision boundaries. Native-Value negative zero also checked owned/borrowed.
 Early extras before requestTime retain nested signed zero and overflow.
- Rust remove-adapter and normalize-zero controls, plus an independent on-disk
 TS numeric-field-to-string control, each fail named assertions at101. Restored
 live passes. Logs:/tmp/bitget-ingress-control-*.log and restored.log.
- Adversarial review MERGE; independently measured90 early-extra cases and1890
 malformed raw/owned/borrowed refusals. Its early-extra and private-token object
 negatives are now maintained. Construct reserved-key Value objects explicitly:
 parsing their text through Value first can promote the private Number object
 before the record decoder is tested.
- Five raw-overflow findings close for these direct production entry paths only.
 Residual UTF8 choices retain divergent rows:131 pending/37 ported/59 divergent.
- Final scoped venue suite29/29 (zero ignored), host strict Clippy and full
 dependency-inclusive wasm strict Clippy separately, parity, layers and local
 extractor zero drift all pass. Extractor clean-install caveat remains open.
 Real tomllib:323 BLOCKS/323 DISTINCT. Logs:/tmp/bitget-ingress-final-*.log.

## Async plan selector

-504 actual-source cases cover both PlaceOrderOpts and existing Order inputs,
 all source order types/sides, zero/negative-zero/NaN/infinities, original resolver
 and selector rejection identity, and precise store/resolve/select call order.
 PlanSelectionOptions retains the original typed reference and Order metadata.
 The separate position resolver/cache and pure selector bodies keep their own rows.
- Maintained source aliases show selector edits/growth reaching store and later
 store edits reaching returned positions; Rust owned snapshots retain neither
 direction. The row cites the shared snapshot boundary and retains UTF8 witnesses.
- Four on-disk Rust/TS controls independently invert the zero branch and opposite
 side. Each fails a named calls assertion at101 and restores green. Mutation
 attempts exposed missing alternate operations in auxiliary source receivers;
 receivers were completed so the final controls fail at behavior assertions.
 Logs:/tmp/bitget-plan-control-*.log and /tmp/bitget-plan-restored.log.
- First adversarial review held the source Order union and positions alias axes;
 both were repaired/witnessed before re-review. Its independent <=0 mutation
 changed60 cases in the initial252-case corpus.
- Across-await options witness: source resolver receives the original symbol
 and opposite side, then caller edits amount/symbol/side while it is pending;
 selection observes all edits through the identical options object. Rust's
 immutable argument borrow excludes that mutation, declared CHOSEN separately
 from preserving reference identity.
- Store notification sweep:37 already-ported targets audited; zero Store::update
 or Store::set_setting crossings. Entire production crate contains no store
 mutation call. Ticker patch construction and position folds return values; their
 store-application rows remain pending. Native Store regression tests do not
 constitute production write-row claims. No reclassification for this seam.
- Final adversarial re-review MERGE after all three boundary findings were
 addressed. Full32/32 venue tests (zero ignored), host strict Clippy and full
 dependency-inclusive wasm strict Clippy separately, parity, layers and local
 extractor zero drift pass. Extractor clean-install caveat remains. Real tomllib
323 BLOCKS/323 DISTINCT;130 pending/37 ported/60 divergent.
 Logs:/tmp/bitget-plan-final-*.log.

## Store notification reachability correction

- The earlier direct-write census was insufficient to establish indirect
 notification reachability. STORE_NOTIFICATION_AUDIT.md records the replacement
 audit following Bybit's send/reconnect/close/health method.
- All37 ported source declarations resolve:19 callable roots and18 data/type
 rows. Fixed call chains include profile receiver fields, the Store market-index
 getter and shared numeric adjustment. None reaches notification. No ported
 send/reconnect/close/health root exists; injected arbitrary callback effects are
 outside this fixed-call claim. Zero affected ported rows, no reclassification.
- This result covers current roots only. Future socket orchestration and Store
 application rows require a fresh reachability check. Until the full notification
 ABI lands, each new crossing is inventoried and classified FINDING, then
 migrated at the compiler-enforced cutover.

## Private-stream order lifecycle caches

- Three source methods now use insertion-ordered native maps. Sixty maintained
 source scenarios pin both TTLs, exact >2000 triggers, pruning both maps, NaN
 retention, replacement/deletion order and ordinary inherited-name Map keys.
 Fresh entries above2000 remain uncapped; an oversized opposite map alone does
 not trigger pruning. Two valid-JSON lone-surrogate witnesses retain the UTF8
 boundary as the sole divergence.
- Twelve independent Rust/TypeScript on-disk controls fail named assertions at
101 and restore green: expiry relation, both TTLs, both thresholds and retaining
 the tombstone when marking open. Logs:/tmp/bitget-lifecycle-control-*.log.
- Adversarial review initially HOLD: compiling shift_remove→swap_remove survived
 because no fixture removed an interior key with multiple survivors. Added
 interior-open-removal; the same on-disk mutation now fails by name at101, then
 restores green. Reviewer independently reproduced that failure and returned
 MERGE. Logs:/tmp/bitget-lifecycle-interior-control.log and restored.log.
- Dependency addition is the already-resolved indexmap2.14.2; lock delta is one
 dependency edge for this crate, with no package, version or feature change.
- Full34/34 venue tests (zero ignored), host strict Clippy and full
 dependency-inclusive wasm strict Clippy separately, parity, layers and local
 extractor zero drift pass. Extractor clean-install caveat remains. Real tomllib
323 BLOCKS/323 DISTINCT;127 pending/37 ported/63 divergent.
 Logs:/tmp/bitget-cache-final-*.log.

## Merge of pinned eefa54442a

- Merged-state adversarial review MERGE, verifying actual parents004b826300 and
 eefa54442a. NamedCI87/88→88, none missing. Real TOML327 BLOCKS/327 DISTINCT;
 zero blank reasons or missing parent targets. Bitget dependencies and lock
 retained, layer containers unchanged. Reviewer scoped locked offline compile
 passed.
- Full34/34 venue tests, host strictClippy and full dependency-inclusive wasm
 strictClippy separately, parity, layers, wiring and local extractor pass. Local
 extractor clean-install caveat remains. Logs:/tmp/bitget-merge-eefa-*.log.
- Integration advanced to1f6698344c during validation; this is an intermediate
 reviewed merge, not a claim of readiness against that newer tree.

## Follow-on merge of pinned1f6698344c

- Independent merged-state review MERGE on d6ea684316+1f6698344c, scoped
 compile green. NamedCI88/88→88; exemptions327/333→333 BLOCKS/333 DISTINCT,
 no parent entry/reason loss. Shared risk changes retained; Bitget has no direct
 calls to the changed risk operations.
- Full34/34 venue tests, host strictClippy, dependency-inclusive wasm strictClippy,
 parity, layers, local extractor and241/241 CI wiring checks pass. Local extractor
 clean-install caveat remains. Logs:/tmp/bitget-merge-1f669-*.log.
- Integration advanced again to7772471f39; final readiness requires its delta.

## Final merge of pinned7772471f39

- Independent merged-state review MERGE on95751f84d1+7772471f39. NamedCI
88/88→88; real TOML333 BLOCKS/333 DISTINCT with all parent entries/reasons.
 Lock retains Bitget/indexmap and adds incoming num/indexmap and GRVT/num edges;
 no lost selectors, dependencies or layer entries. Scoped all-target compile
 passed.
- Full34/34 venue tests (zero ignored), host strictClippy and dependency-inclusive
 wasm strictClippy separately, parity, layers, local extractor zero drift and
241/241 CI wiring checks pass on this tree. Extractor clean-install caveat
 remains. Logs:/tmp/bitget-merge-777-*.log.
-127 pending/37 ported/63 divergent. No claim that the pending ticker handler
 or its prototype-write/notification paths are implemented by this merge.

## Public and secondary ticker handlers

-318 actual-source cases execute both real class bodies against the real Store
 constructor/update path. Snapshots tag numeric bits before JSON, including
 derived infinity and signed zero. Both handlers use native ModelPatch.
- All12 inherited names are witnessed with and without an id="undefined"
 sentinel. Shared TableHit retains the lookup branch; shared String(undefined)
 reproduces the source Store property key coercion. Pre-serialization type/id,
 write target, stored state and notification are captured together.
- Notification FINDING pins commit before first subscriber throw, skipping the
 second subscriber and remaining batch rows. Current Rust infallible Subscriber
 continues both rows; the audit inventories2sourcepaths/1Rustcall. Dictionary
 WRITE FINDING separately pins a __proto__ ticker poisoning the lookup prototype
 so a later symbol key targets the sentinel. No production panic workaround.
- Nine on-disk controls: Rust own-only lookup, symbol priority, JSON numeric
 patch; independently on each TS class own-only lookup, symbol priority and
 derived-finite guard. All compile and fail named assertions at101; restored
 live green. The first TS own-only attempt failed at an unnamed fixture unwrap;
 replaced it with a named write-boundary assertion and ordered positive sentinel
 cases first so the final control fails at the wrong-record state assertion.
 Logs:/tmp/bitget-ticker-handler-control-*.log and controls-restored.log.
- Native NaN is an explicit input-domain witness (it blocks a later bid alias);
 it is never converted to null. UTF16 witnesses preserve legal source surrogate
 keys and the shared Rust representation refusal. Scoped frozen/live2/2 pass.
- Adversarial review MERGE; independently replayed frozen/live2/2 and measured
 a source skip-unchanged-updates mutation changing31 public cases, including
 same-value-still-notifies and signed zero. Historical no-write audit wording
 now explicitly refers to pre-handler fa86d4ef33.
- Final narrow re-review MERGE after the shared owner clarified the stock
 dictionary contract excludes already-poisoned dynamic reads. Notes state both
 initial replacement and later sentinel write; no shared repair is claimed.
- One production Store write, zero inferred map_err conversions. Its concrete
 From<StorePatchError> retains the typed payload and forces deliberate handling
 of the notification ABI change. Reviewer independently verified that boundary.
- Full36/36 venue tests (zero ignored), host strictClippy and dependency-inclusive
 wasm strictClippy separately, parity, layers and local extractor zero drift pass.
 Clean-install extractor caveat remains; real TOML333 BLOCKS/333 DISTINCT.
125 pending/37 ported/65 divergent. Logs:/tmp/bitget-ticker-handler-final-*.log.

## Bill-page parsed read (2026-10-13)

`_fetchBillPage` executes 1,071 cases from the actual TS page body and income
mapper. Native request/output numbers are compared before serialization; NaN
payload bits are not a JS observable and are canonicalized, while signed-zero
bits remain exact. Original read/resolver failure identities, public guard,
nullish id fallback, truthy option guards, request snapshots in both directions,
UTF-16 refusal and inherited category output are explicit. No row claims HTTP
assembly or the outer error-swallowing pager.

Eight on-disk compiling controls fail named assertions at exit 101 and restore:
Rust NaN-min, limit-nullish, id-truthy; TS limit-nullish, id-truthy,
time-presence; and Rust/TS resolver-before-limit. The first NaN mutation stopped
at an unused-import lint and was NOT credited; the replacement compiled and
failed the request assertion. Reviewer initially HOLD: early resolution survived
796 independent-field cases. Conflicting-input fixtures increased the corpus to
1,071; the exact source mutation and analogous Rust mutation now fail named
limit-before-resolver cases. Reviewer independently measured six affected cases
and returned MERGE. Scoped frozen/live/drop tests 3/3 pass. Full merged-state
gates follow the integration hold release; this entry does not claim them yet.
Logs: /tmp/bitget-bill-control-*.log and /tmp/bitget-bill-order-restored.log.

## Integration e92bdd9b04 merge validation (2026-10-13)

Parents 7c40c5c23e / e92bdd9b04 merge without textual conflicts. Named CI step
multisets 88 / 90 become 90, none missing. Real tomllib exemption parses reconcile
333 / 338 to 338 blocks / 338 distinct targets, no missing entries or blank
reasons. Bitget Cargo.toml is unchanged. Initial jobs-2 all-targets check could
not spawn rustc (OS error 35, resource temporarily unavailable); jobs-1 retry
passes. All following gates run serially with CARGO_BUILD_JOBS=1:

- Venue oracle: 39 passed, zero failed/ignored, including all live source tests.
- Host strict Clippy: pass.
- Dependency-inclusive wasm strict Clippy: pass, separately measured.
- Global parity, extractor drift and layer audit: pass.
- CI wiring: 251/251 pass, zero skipped.

Extractor result is local; the previously recorded clean-install dependency
caveat is not discharged by this run. Incoming coinbase formatting repair is
present through 18261b604c. Counts remain 124 pending / 37 ported / 66 divergent.
Logs: /tmp/bitget-merge-e92-*.log. Merged-state adversarial review follows; this
entry records gates, not a review verdict or merge-up readiness.

Merged-state reviewer returned MERGE for 602a1dae63. One exemption reason is
legitimately updated to incoming's fuller js_number_string backend explanation;
no claim is made that every reason is byte-identical to both parents.

The next integration delta, pinned 57c7839dc1, changes only the spec and
Binance/Lighter-owned files. Bitget, lock, layers, workflow, wiring and isolated
selectors are byte-unchanged. Step multisets 90/90→90 and parsed exemptions
338/338→338 blocks/distinct preserve both parents. Scoped all-targets check,
global parity, zero extractor drift and 251/251 wiring pass. The full 39-test
venue and separate host/wasm lint results above cover unchanged code and
dependencies; they were not redundantly rerun for this disjoint delta.

## Refusal timing audit (2026-10-13)

Added 84 actual-source cases: 36 linear and 36 inverse attached-leg cases
require the later coercion TypeError rather than an early typed-output refusal;
12 whole bill-page cases require the later null-row TypeError after an inherited
category. Assertions retain exact error name/message and operation traces.
Pending outer order-batch callers are explicitly outside this audit.

Four on-disk compiling controls fail named assertions at exit 101: linear early
projection, inverse early projection, inherited-income early refusal, and source
null-row filtering. Every control was restored; frozen/live order and bill tests
pass 12/12. An independent adversarial review reran all 12 and returned MERGE.
Global parity and extractor drift pass; the latter is local validation and does
not discharge the clean-install caveat. Production code and row statuses are
unchanged: 124 pending / 37 ported / 66 divergent.
Logs: /tmp/bitget-refusal-timing-*.log.

## Income history catch boundary (2026-10-13, candidate)

The actual fetchIncomeHistory class member runs over 151 cases: synchronous page
throw and asynchronous rejection, nullish versus falsy message selection,
coercion throwing inside catch, reporter failure identity, and unchanged success
page ownership. Original options are forwarded by the Rust function, with an
actual pointer-identity assertion. No trim is applied to diagnostics. Shared
UTF-8 refusal has two double-escaped source witnesses. Dropping the wrapper
drops its pending page operation without invoking the reporter.

Four on-disk compiling controls fail named outcome/effect assertions at exit
101: Rust and TS truthy message fallback, and Rust and TS reporter-error
suppression. Restored tests pass 3/3, including the live oracle; options forwarding
was subsequently made explicit and all three pass again. The first control
runner expected an outcome assertion but observed the correct named effects
assertion instead; the runner was widened to accept either named assertion and
all four controls were rerun successfully. Logs: /tmp/bitget-income-history-*.log.

This is a supplied-operation boundary, not a local emitter implementation or a
claim that the currently unit-returning BaseExchange reporter can propagate
listener throws. Core has received the corpus for its fallible emitter migration.
Candidate counts: 123 pending / 37 ported / 67 divergent. Final review and gates
are recorded below when returned.

Reviewer HOLD found a real survivor: clearing a successful page in place left
the entire 151-case oracle unchanged because successes were empty. Six populated
success cases now preserve row order, all fields, and native income/timestamp
bits including signed zero, NaN and both infinities (157 cases total). Both the
source in-place clear and Rust Vec::clear controls are maintained. The reviewer
also identified a zero-sized listener-error identity witness; it now uses a
nonzero-sized allocation with a distinct marker and metadata, asserting both
contents and the original allocation identity. These were verification gaps,
not a change to the catch implementation.

Final corpus also adds positive scalar Unicode, an astral pair and NUL alongside
BOM/NEL: 159 catch cases plus six populated successes, 165 total. Reviewer
independently killed the source clear mutation on all six new cases and returned
MERGE, including the final Unicode-only delta. All six compiling controls fail
named assertions at 101 and restore. Full scoped venue gate: 42 passed, zero
failed/ignored. Host strict Clippy and dependency-inclusive wasm strict Clippy
pass separately; parity, local extractor drift and layer audit pass. Real tomllib
parse: 338 exemption blocks / 338 distinct. Counts: 123 pending / 37 ported / 67
divergent. Logs: /tmp/bitget-income-final-*.log.

## Type inventory inheritance guard (2026-10-13)

Before repair, an on-disk valid source mutation made FETCH_POSITIONS_V2_RESPONSE
extend a namespaced ExtraNumeric with required hiddenNumeric:number. The
maintained oracle still exited 0 and reported 9 types / 2034 shape cases. The
namespace keeps the inherited declaration outside the direct declaration census,
so a new top-level type count cannot mask the missing heritage traversal.

The interface branch now refuses any heritage clause by declaration name. A
permanent live test injects the same source before the actual AST reader and
requires that refusal; it does not call a synthetic shape helper. On-disk source
inheritance now fails the normal live oracle at named exit 101; independently
removing the guard fails the new negative test at named exit 101. Both restore
green (3/3 scoped type tests). Current source uses no interface inheritance, so
no row reclassification is implied: 123 pending / 37 ported / 67 divergent.
Logs: /tmp/bitget-heritage-{before,source-control,guard-control,restored}.log.

Post-resume independent reviewer returned MERGE: removing the guard in memory
makes the real-source probe succeed, and scoped frozen/live/heritage tests pass
3/3 with jobs 1. No production shape or row status changed.

## Resume merge-down to a3d514bec8 (2026-10-13)

Verified a3d514bec8 on origin before merging. Parents ab2a24cde8 / a3d514bec8
retain 90 / 90 named CI steps as 90, with zero missing multiset entries. Real
tomllib parses retain 338 / 338 exemption blocks as 338 blocks / 338 distinct
targets, zero missing parent targets and zero blank reasons. The Bitget crate
dependency manifest is byte-identical to both parents. No textual conflicts.

All gates run serially, CARGO_BUILD_JOBS=1: scoped all-targets compile passes;
43 venue tests pass with zero failed/ignored; host strict Clippy passes; separate
dependency-inclusive wasm strict Clippy passes; parity passes at 7334 rows /
3234 public Rust items; extractor reports zero manifests updated, zero added,
zero orphaned (local dependency-install caveat unchanged); layer audit and CI
wiring pass. Counts remain 123 pending / 37 ported / 67 divergent.
Logs: /tmp/bitget-merge-a3d-*.log. Merged-state review follows these gates.

## Bill-page NaN capture across architectures

Run 38345331756, job 115125597863, used Linux x86-64 Node 22.23.1
at 5d55c8ec6e (Bitget files unchanged at a3d514bec8). Its raw log reports:

```
thread 'live_bill_page' (3562578) panicked at safe-cex-venue-bitget/tests/bill_page.rs:258:5:
assertion `left == right` failed: bill page source drift
```

Parsing both complete Debug values found exactly 18 differing
`cases/*/calls/*/params/limit/bits` leaves: CI `fff8000000000000`, frozen
macOS arm64 `7ff8000000000000`, all tagged NaN. No other leaves differed.
The actual mapper comparison passed; the uncanonicalized final snapshot failed.
This was an oracle defect, not a manifest or shared implementation regression.
The exact a3d514bec8 Yarn script passed locally on arm64 Node 22.23.2 with
CI's `RUSTFLAGS=-Dwarnings`; that is not a Linux reproduction.

The capture now canonicalizes only NaN to `7ff8000000000000` on both sides.
All non-NaN bits remain compared, including negative zero. The fixture header
records the architecture witness; its original cases remain unchanged. No
tracked artifact outside this crate consumes the bill-page fixture.

Three independent on-disk capture mutations each fail the live Rust test at
exit 101: removing canonicalization names `numeric identity negativeNaN`,
collapsing signed zero names `numeric identity negativeZero`, and replacing
1.25's bits with 1.5's names `numeric identity finite`. Restored probe passes;
the scoped bill_page binary passes four tests with none ignored.
Independent sub-agent verdict: MERGE. Its compiling Rust capture mutation
collapsed negative zero, failed at exit 101 naming `bill page Rust numeric
identity negativeZero`, and restored to a passing probe.


## Native field identity supplements

The independent reviewer compiled reciprocal serde-name swaps in production:
locked/available passed frozen_types_and_protocol (exit 0), but failed the new
native test (101) naming FETCH_SINGLE_ACCOUNT_RESPONSE/data/locked. A second
USDT/USDC variant-name swap also passed the old test and failed native category
identity USDT-FUTURES (101). Both restored to passing. The inventory is explicit
native Rust members, not a serialization generated from the same serde metadata:
178 fields across 17 records and three category variants, with source-derived
optional/null policies and raw/owned/borrowed populated samples.

A compiling ticker bid/ask destination swap failed the field-identity:all state
assertion (101), restored green. An order price/amount projection swap already
failed the existing unknown-type-strategy case (101); that pair was not a new
coverage gap. Reviewer caught an initial fold supplement that checked only
pointer origins; it now also compares decoded input members directly before
selection, retaining the origin assertion for output identity.

All identity supplements are live source modes alongside the original fixtures;
none rewrites a frozen artifact or claims retroactive independence. Broader
candidate classification is recorded separately in FIELD_IDENTITY_AUDIT.md.

The async plan supplement records source inputs before execution and reads nine
native Position fields on store and resolved paths. A compiling Rust
entry_price/notional swap before selection passed frozen_plan_selection (0)
but failed plan_selection_field_identity (101), naming `plan native position
field identity entryPrice 0`. The actual TypeScript callback-boundary swap
independently failed the same assertion at 101. Both restored to passing.

Subprocess audit: live captures invoke Node once per complete corpus, not per
case. The order capture invokes each of its two distinct child corpora once.
Mutation controls rerun a complete corpus per mutation; they do not spawn per
fixture row. All package gates in this session used the real Yarn entrypoint.

Final independent sweep review: MERGE, with no blocking findings in the documented
current-owned scope. Host strict Clippy and separate dependency-inclusive wasm
strict Clippy pass for the four changed test targets, serially with --jobs 1.
Real TOML counts remain 123 pending / 37 ported / 67 divergent. These are slice
gates, not a new merged-state readiness claim.

## Historical fills orchestration

The maintained capture executes fetchHistoricalFills, mapBitgetFills and
splitTimeRange from their actual TypeScript declarations in one Node process.
Native request and execution numbers are tagged before serialization; NaN alone
is canonicalized, while all other bits remain compared. Ordered traces include
detection, UTA getter, market resolution, per-page product getter, reads and
raw error reporting. The shared splitter supplies windows; there is no local
splitter, retry policy or task registry substitute.

Four compiling on-disk controls failed at exit 101 and restored to passing:
Rust short-page termination named historical fills output short-page-continues;
Rust timestamp-floor removal named historical fills output baseline;
TypeScript resetting seen at each window named historical fills output
window:0:7689600001; TypeScript request PAGE_LIMIT reduction to 99 named
historical fills effects baseline. The last control changes request size,
not the separate MAX_PAGES backstop. The window control failed at an earlier
valid witness than the originally selected global-window-dedup case.

Two cancellation paths explicitly drop pending detection and page operations.
No future is spawned locally. Source option mutation during detection and lone
surrogate symbols remain representation witnesses, not silently sanitized
inputs. Concrete shared error reporting remains a FINDING at this base; the
supplied fallible reporter's error is propagated rather than treated as [].

Independent review expanded the oracle to 87 cases with derived fee overflow
and both zero signs. It added escaped detection/resolver/reporter metadata
equality and original allocation identity, plus read-error metadata at the
projection boundary. Its exact TypeScript MAX_PAGES 100-to-99 mutation failed
at 101 naming historical fills effects cursor-cycle-cap, then restored all
three scoped tests to passing. This is separate from the request-limit control.

Review verdict MERGE. Final live target passes after moving source config capture
before execution; scoped target has three passing tests. Host and separate
dependency-inclusive wasm strict Clippy pass, serial --jobs 1. Layer audit passes
78 checks. Parity passes 7334 rows / 3235 public Rust items after adding the exact
relocated Rust target to the row note. Extractor reports zero changed manifests,
zero added/orphaned symbols (existing local-install caveat retained). Exempt TOML
parses as 338 blocks / 338 distinct. Cargo.lock changes only Bitget's dependency
on the already-resolved candle-cache crate. Counts: 122 pending / 37 ported /
68 divergent. This is slice validation, not merged-state readiness.
PR 7234 follow-up: the workflow's `yarn venue-oracle:bitget` already expands to
`cargo test ... -- --include-ignored` in package.json. The numeric probe is now
also non-ignored, so ordinary Cargo test executes it. Its existing encoder and
probe run before TypeScript loading; it requires Node only. An invalid compiler
path leaves all eight probe patterns intact, while ordinary source capture
still fails with MODULE_NOT_FOUND. Missing Node fails with the named diagnostic
`bill page numeric capture requires Node on PATH`; there is no skip or fallback.

## Integration merge-down from 5b3f63f2ea

Parents: Bitget 1a961fb65ae3a486eb68815543db07d7e7a9d45b and integration
5b3f63f2ea7af6b63c5c37d1763cf52b77357ad0. Named CI step multisets are
90 / 90 -> 90, with no missing entries from either parent. The incoming push
trigger is exactly master and feat/safe-cex-rust; feature-lane PRs no longer
receive an additional push matrix. Bitget Cargo.toml, Cargo.lock, layer policy
and isolated dependency selectors remain byte-identical to the lane parent.
Real TOML parsing yields 338 exemption blocks / 338 distinct targets, no blank
reasons, duplicates or entries missing from either parent.

Four conflicts were resolved per hunk. The incoming Node-only numeric probe
remains non-ignored with a named missing-Node error. The encoder/probe appears
once before TypeScript loading. Fixture merge checked all 1071 common cases by
name and value, not prefix position: the lane has 12 later inherited-then-null
cases interspersed before other cases. The merged fixture retains all 1083;
shared metadata and all non-case axes are identical. Evidence prose retains
both lane slice history and the narrow repair's independent failure controls.

Capture environment for this merge validation: macOS/darwin arm64, Node 26.7.0
(major 26). The separate recorded CI NaN witness remains Linux x86-64, Node
22.23.1 (major 22); this run does not retroactively label an older frozen capture
with a Node version that was not recorded at capture time.

Merged-tree gates, serial CARGO_BUILD_JOBS=1: locked crate compile passes;
venue-oracle:bitget passes all captures and 51 tests with zero failed/ignored;
host strict Clippy and dependency-inclusive wasm strict Clippy pass separately;
crate-scoped fmt passes; parity passes 7334 rows / 3235 public Rust items;
extractor has zero changed/added/orphaned entries (local-install caveat retained);
layer audit passes 78 checks and CI wiring passes 251. All package scripts use
the real Yarn 1.22.21 JavaScript entrypoint. Counts remain 122 pending / 37 ported /
68 divergent. Logs: /tmp/bitget-merge-5b-*.log. The merged-state review will name
the resulting merge SHA rather than transferring a verdict from either parent.

## Store notification cutover (integration 9419b340d8a6)

The sole production mutation, shared by both public ticker handlers, failed
compilation at ticker.rs:129 with E0308: expected Result<(), StorePatchError>,
found Result<(), StoreMutationError>. The direct cutover retains the complete
StoreMutationError in TickerHandlerError, including Notification's Store-local
subscriber ID, response metadata and Rc cause identity. No ExchangeError
conversion is used: that type has no store_subscriber_ids at this integration
SHA. Concrete socket assembly and core structural identity retention are not
claimed repaired.

The original ticker-handler fixture bytes remain unchanged. Frozen and live
handler tests plus the field-identity supplement pass 3/3; the existing throwing
subscriber cases now assert exact source state and listener order instead of
pinning Rust continuation. The first row commits, the failing callback sees that
state, and later listeners and batch rows do not execute. The separate poisoned
prototype-chain FINDING remains. Order's existing non-finite Store witness also
passes after handling setup mutation Results (frozen_order_decisions, 1/1).

Two on-disk production controls compile and fail at exit 101:
- Deferring the first Store error until after the batch fails
  `ticker notification stops later listeners and rows`.
- Rebuilding the Notification cause from its message, retaining its ID, fails
  `ticker notification original cause identity and response`.

The first draft of the deferred-error control had an E0271 conversion error and
is NOT runtime evidence. Its corrected compiling version produces the named
failure above. Both controls were restored and frozen_ticker_handlers passed.
Capture execution here: darwin arm64, Node 26.7.0 (major 26); no frozen capture
was regenerated and no older capture's Node version is inferred.

The merge retained all 90 named CI steps from each parent, and a real tomllib
parse reports 359 exemption blocks / 359 distinct targets, zero blank reasons
and zero parent omissions (lane parent 338/338, integration parent 359/359).
The manifest remains 122 pending / 37 ported / 68 divergent: repaired notification
causes leave the two rows' independent representation and poisoned-chain causes.

Scoped cutover gates: host strict Clippy and dependency-inclusive wasm strict
Clippy both exit 0 separately; parity exits 0 (7334 rows / 3288 public items);
layer audit passes 78/78. All package scripts use the real Yarn JS entrypoint.
Extractor exits 1 with `Would update 7 manifests; 7 added, 0 orphaned symbols.`
The named files are base.ts, base.ws.ts, phemex.api.ts, phemex.types.ts,
unknownSendOutcome.ts, ChaserOrderManager.ts and ChaserOrderOptions.ts. Their
source, manifest and extractor paths have an empty diff against 9419b340d8a6;
this is inherited integration drift, not a Bitget manifest edit. It was routed
with the exact failure text; no other owner's manifest was repaired here.

Independent reviewer control: suppressing updateTicker's throw in the actual
TypeScript public handler fails live_ticker_handlers at exit 101, named
`source notification error surfaces`; exact source bytes restored and test passes.
A separate native supplement adds first-success/second-failure fanout, pinning
subscriber ID 1 within that Store plus cause identity, commit and skipped rows.
Hardcoding notification ID 0 compiles and fails at exit 101, named
`ticker second subscriber native identity`; restored test passes. The expanded
ticker target passes 4/4 including both live-source tests; the original frozen
artifact remains byte-identical.

## Landed subscriber identity (integration 9af82bfeae)

Fetched origin and inspected ExchangeErrorMetadata plus both runtime conversion
append sites before adopting. The existing second-subscriber witness now consumes
the shared fallback projection and asserts [1] plus the original response. Bitget
retains StoreMutationError directly; there is no temporary box to remove and no
production ExchangeError conversion or socket assembly claim. The outside-handler
core subscriber-ID finding recorded above is closed on this landed contract.

Cutover collection iterations: initial check found E0308 at ticker.rs:129; after
migration strict host and wasm all-targets checks found zero new type or unused
Result sites. The third collection on this newer base again finds zero on both
targets. Test setup handles three writes explicitly, including two position
insertions; production still has one mutation call serving two source rows.

Merged-tree venue oracle passes 52/52, zero ignored across 15 test binaries.
Strict host and dependency-inclusive wasm Clippy pass separately; parity passes
7338 rows / 3322 public items, and crate fmt passes. Both parents retain all
90 CI steps; exemptions remain 359 blocks / 359 distinct, no missing or blank
reasons. No full workspace suite was run.

Extractor remains inherited red: `Would update 4 manifests; 3 added, 0 orphaned
symbols.` It names bingx.api.ts, unknownSendOutcome.ts, ChaserOrderManager.ts and
ChaserOrderOptions.ts. Those source/manifest paths and the extractor have zero
diff against 9af82bfeae. The earlier base/base.ws/phemex drift is no longer present.
This sheet is not a claim of merge-up readiness while extractor remains red.

## Historical-position delegation

`fetch_historical_positions` calls the already-public shared journal fold with
symbol_scoped=false. The slice owns Bitget's delegation, not the shared derivation
or packages/venue-data grading; no raw axis is independently pronounced trusted.
Concrete Exchange override assembly stays pending. A live source script executes
the actual Bitget field, actual BaseExchange fold and actual capability predicates
once per corpus, injecting only the journal operation and deterministic clock.

The corpus has 64 distinct from/to pairs: missing, signed zeros, finite positive
and negative, both infinities and NaN. Original inputs are captured before source
execution; native argument bits are read before serialization, canonicalizing only
NaN. Accounts differ per case. Native result Vec and error metadata allocation
identity are pinned along with structured response/subscriber metadata and dropping
the pending journal future. Source options are forwarded by reference; record alias
changes are witnessed in both directions, and callable/own-undefined record fields
are observed before JSON omission. These retain explicit owned/JSON/UTF8 boundary
notes, not a claim that serializing the result proves full output identity.

Two scoped tests pass including the live oracle. Four compiling on-disk controls
fail at exit 101 and restore to 0: Rust symbol scope false→true and Rust from/to
swap fail `historical positions native arguments`; Rust error flattening fails
`historical positions original structured failure`; actual TypeScript scope
false→true fails `historical positions native arguments`. Runtime is darwin arm64,
Node 26.7.0 major 26; the live header records these values per capture. No generated
corpus or frozen fixture was authored. Candidate count: 121 pending / 37 ported /
69 divergent; only the delegated historical-position row moves.

Reviewer independently changed the delegated result to clone its Vec: it compiled
and failed at exit 101, named `historical positions moves original folder records`;
restoring the source passed. Native receiver identity now compares the original
BaseExchange address, not just its class name. No new inert-capability or settlement
lookup coverage is claimed; those are shared-fold responsibilities, outside this
Bitget delegation slice. Scoped host and wasm strict Clippy pass separately.
The manifest gate required the exact adapted Rust path in the note; adding it
repairs that registration assertion without changing the implementation.


## Ticker prototype lookup and primitive ID writes — measured c55da80b19

Measured code SHA `c55da80b19f55670ecbe79b37c27b442297ee0fc`; original integration parent
`d74a1c01f77439f9df61fa330063be194aab7576`, merged integration parent
`333fd419fdbf70f081039610f22da9ee653bc96f`. Pushed before merged gates. No separate
unreviewed feature branch ancestor. Exact exits and census are in
`TICKER_PROTOTYPE_EVIDENCE.json`; the later evidence-only leaf is not claimed built.

At the measured SHA: scoped Store and Bitget all-targets checks (`--jobs 2`) each0;
`ticker_prototype`1passed/0ignored exit0; `ticker_handler --include-ignored`4passed
exit0; strict host Clippy for those two Bitget test targets0 and Store lib0;
extractor check0, output `Would update 0 manifests; 0 added, 0 orphaned symbols.`;
scoped rustfmt0. The gate wrapper itself returned1 because it expected writer
wording `Updated` in check mode. This harness-label failure is retained; actual
extractor exit/output were read, and the remaining scoped format checks ran0.

All33 native scenes finish before either source process. Handler floor22 through21
notification-throws is reused against both real source handlers (44 source
evaluations); direct Store floor11 through10 noncanonical-index gives55 source
evaluations total. Native decoded fields/bits, subscriber counts and cause identity
precede projections. Direct errors additionally assert public name, failed field,
exact message and actual Display. Existing159 cases per handler retain data/effects.

At earlier pushed `b0c3372d3b1948772c152d1c19213e3da7709127`, four ISOLATED compiling
Rust controls each failed101 at their named native assertions: lost prototype
lookup, NaN truthiness, missing primitive-ID error, public error-kind loss; each
restored0. Four source-result collection controls (empty/subset for22 and11 floors)
each failed1 at independent counts and restored0. These are collection-discard
controls, not claims of empty production input coverage. At merged measured SHA,
an independent Display-prefix mutation compiled and failed101 at
`PROTO native public mutation Display`, then restored0. The separate pre-repair
direct Store diagnostic failed101 at6efabdbf928acfa9e4ed98291ca4381a9fd8497f while
actual source0; it was repaired and flipped positive, not dropped or ignored.

Adversarial sub-agent reviews (`account_read_review`, `bitget_findings_audit`)
found no blocker at6a5001133f and focused merge-delta review at measured c55da80b19.
The Display proof gap was fixed and discriminated. Unknown `__proto__` patch setter
effects remain explicitly outside the known-field floor under the priced typed
patch policy. Both borrowed getter shapes are divergent CHOSEN, with native own-key
versus actual source prototype observations. No blanket dictionary repair claim.

Tomllib census: Bitget142ported/85divergent/0pending unchanged; this slice closes
two handler finding notes, moves0pending and0toported. It reclassifies two Store
getters ported→divergent and repairs the newly discovered direct Store defect.
Integration separately supplied five original policy closures; one reporter finding
remains separately prepared in PR7865. The two existing Bitget CORRECTED market
policies remain unchanged; their284-case floor is earlier evidence, not rerun here.
Canonical declarations are referenced only at
[14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Not run: full Store or Bitget suites, workspace compilation/tests, all-target host
Clippy, wasm gates, global parity checker, CI or live exchange tests.

## Concrete reporter availability — code 4bcbcbd6cc6209d1a9b2a30d068e6e623a6962cd

Parent actually built on: ac0f5fccf24207b26c279e67da2a88f2c72a7aae,
current integration at branch creation; no unmerged Bybit work is an ancestor.
The earlier unit-returning shared reporter API claim is stale. A new witness
calls historical fills through actual BaseExchange string-message reporting.
Native assertions check original metadata allocation, complete metadata,
response/context, error name/message and listener short-circuiting before any
wire assertion. General raw-event ingress and concrete exchange/HTTP assembly
are not established. Canonical boundaries remain at
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

At the code SHA above, measured exits:
- scoped Bitget `cargo check --all-targets --jobs 2`: 0;
- `cargo test --test concrete_reporter --jobs 2`: 0, one native test;
- `cargo test --test order --jobs 2`: 0, one frozen test, nine ignored;
- targeted `live_order_decisions -- --include-ignored --nocapture`: 0,
  one live-source test, nine other tests filtered out;
- strict Clippy for `--test concrete_reporter --jobs 2`: 0;
- extractor writer and extractor check: 0, zero changed/added/orphaned rows;
- scoped rustfmt check for the new test: 0.

Three independent on-disk production mutations compiled and exited 101:
swallowing `report(message)?` fails
`concrete reporter listener failure must escape historical fills catch`;
rejecting constructor market fallback fails
`CORRECTED inherited non-Market lookup`;
returning an empty constructor margin coin fails
`CORRECTED inherited .base yields undefined in TS`.
Every original source byte was restored; reporter and frozen order targets each
then exited 0. There were no surviving controls or compiler rejections in this
three-control floor, stopping at corrected-margin-coin. The existing inherited
native witness floor is 284 cases; this is not exhaustive prototype coverage.
The two CORRECTED policies remain unchanged.

Independent tomllib census: 142 ported / 85 divergent / 0 pending, 227 rows;
0 pending-to-ported, 0 pending-to-divergent. FINDING-bearing notes fall 8 to 7.
The seven remaining notes concern inherited object outputs, native order-info
NaN and replaced Store prototype chains; this slice does not close them.
Read-only sub-agent `/root/bitget_findings_audit` reviewed the exact code SHA:
no blocking issue for the narrow availability closure; no reviewer Cargo or
mutation claim. Review and root controls are posted on PR7865.

Not run: full Bitget suite, all ignored order enumerations, crate-wide Clippy,
workspace compilation/tests, wasm checks/Clippy, CI, or live exchange calls.

## Reporter repair integration merge — measured47f406e36e

Merged integration333fd419fdbf70f081039610f22da9ee653bc96f and pushed first.
Measured `47f406e36edcaafc83b000d3e7e6dd3bebb3a80b`: Bitget all-targets check0;
concrete_reporter target0; frozen_order_decisions filter0; live_order_decisions
including ignored0; strict host Clippy for reporter/order test targets0; extractor
check0, output Would update0manifests/0added/0orphaned. Exact receipts in
REPORTER_MERGE_EVIDENCE.json. No full suites, all-target Clippy, wasm, global
parity checker, CI or live gates run. Old three mutation controls remain measured
only at4bcbcbd6cc6209d1a9b2a30d068e6e623a6962cd, not rerun or relabelled.

Root/reviewer scoped preservation diff0 for history_reads.rs, market.rs and
concrete_reporter.rs against that measuredcode. Adversarial sub-agent
account_read_review verified original error metadata/cause, first1/later0/page1
proof remains one concrete string-message case; reporter+two CORRECTED notes and
actual TS/shared emitter inputs unchanged. No unrelated incoming policy proof
attributed to this branch. This evidence-only leaf is not claimed compiled.
0pending moved; reporter remains divergent with its separate priced boundaries.
