# Income/history and metadata helper gates

Fresh branch parent: `076f1ae3492a5c5acf63b637fea6da5cd008cb5f`.
Implementation/control base: `7a1a4601e1c84c68aa031aa2fff0a6f36b239ee3`.
Restored gate SHA: `ed98455f4cb3330f1d0f9024109b6585b909fe16`, merging
integration `61d416782d630e0058f93a515e81fafc9be87187`.
No unrelated unreviewed feature branch was used as a parent.

Own change: eight pending→ported, zero→divergent. Python3.14 tomllib compared
per-symbol statuses in all nine assigned manifests with the original parent.
Assigned nine-file census is `(119,0,42)` → `(127,0,34)` in
ported/divergent/pending order. Including the two already-resolved spot constants
and errors files adds six ported rows: `(125,0,42)` → `(133,0,34)`.
Income alone is `(0,0,6)` → `(6,0,0)`; startup helpers `(1,0,2)` → `(3,0,0)`.
Whole Hyperliquid at the restored gate SHA is `(344,106,81)`; its change includes
ten additional ported rows from the other lane. Perp-core TypeScript/manifests
were inherited solely via integration merge, not edited by this slice.

## Implementation and finite witness

Income bodies preserve String/parseFloat finite fallback, nested funding nullish
precedence, fee/rebate signs, transaction ID fallback and derived provenance,
entryPrice=0 and fill granularity. Pagination preserves inclusive cursors,
SameValueZero ID dedupe, original row/callback references, Math.max cursor
semantics, optional cap callbacks and fractional/zero/negative page bounds.
Metadata bodies reuse the request body/config, call the existing Rust tuple
parser, preserve original meta/context references in fresh tuples and retain
logger exceptions and retry/catch order.

The three async exports enter through an eager owned task boundary. Host clones
retain one realm; task registration polls the prefix now and owns continuation
independently of observers. Local async fetch results pass through native
Promise resolution and outer await boundaries, including synchronous HTTP
throws. Return adoption is a mandatory host binding. Browser/native JS calls,
coercion, property/iterator operations, imported tables and task scheduling remain
explicit bindings. No shared SDK type/trait was changed by this slice.

The complete unchanged income/startup TypeScript modules run in the oracle.
PositionSide and endpoint/info declarations are selected from actual source,
plus the actual logging enum. Five source files are SHA-256 hashed by the oracle.
Independent floor:64 cases, ending at index63 (`hip3/logger-catch`):12 numbers,
6 normalized funding rows,8 fill mappings,6 funding mappings,9 derived cases,
9 pagination cases,7 core fetches and7 HIP-3 fetches. Nine Rust tests inspect
native outcome tags, UTF-16, own field order, finite binary64 bits, NaN class,
returned values, ordered effects, original resolver/row/callback/member handles,
fresh tuple identity, body/config identity, eager prefixes and three dropped
observer continuations. There are no re-encoded Rust result comparisons.

The floor includes patched startsWith/includes and a custom lowercase receiver,
unpaired UTF-16, maker rebates, parseFloat prefixes, nested funding fallback,
inclusive boundary overlap, fractional/zero/negative caps, a genuine throw of
undefined, malformed/rejected metadata and throwing retry/catch loggers.
General exotic coercions/proxies/sparse or custom iterators, arbitrary thenables,
full event-loop interleavings, unhandled-rejection integration and live networking
are outside this finite witness. Native primitive backend implementation is not
an independently proved full JS engine.

## Isolated mutation controls at the implementation base

Commands used Node24.20.0 and
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`:
`SAFE_CEX_REST_MUTATION=N cargo test --offline --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --test rest_helpers TEST --jobs 2 -- --exact`.
One selector was active per command, so failures cannot mask other controls.
Temporary selected-patch SHA-256:
`dbc7d09eb12aedf21dbcb46b9c5d43ca228780cd930f6ed3a08ccf1f7f90f745`.

| N | Changed call site | Test / named assertion | Exit |
|---|---|---|---|
| 1 | to_number replaces finite parsed value with0 | finite_number_prefix / REST number "number-4" | 101 |
| 2 | read_funding_row ignores nested delta | nested_funding_rows / REST read "funding-row-1" | 101 |
| 3 | fill mapping emits fee instead of -fee | fill_income_signs / REST fill "fill-0" | 101 |
| 4 | funding transaction suffix becomes :fee | funding_income_ids / REST funding "funding-0" | 101 |
| 5 | derived notional adds price/size | derived_close_records / REST derive "derive-0" | 101 |
| 6 | pagination advances newest+1 | inclusive_history_pages / REST effects "overlap" | 101 |
| 7 | core fallback repeats primary client | core_metadata_fallback / REST effects "malformed-fallback" | 101 |
| 8 | HIP-3 runs only attempt1 | hip3_metadata_retry / REST hip3 "malformed-retry": native array Null | 101 |
| 9 | fixture parse_float returns Err(undefined) | finite_number_prefix / REST native outcome REST number "number-0" | 101 |
| 10 | owned task withholds its synchronous prefix | inclusive_history_pages / REST eager prefix "empty" | 101 |

All ten were compiled test failures at the named assertions; no survivor.
An earlier staging attempt tried returning Value=undefined as generic Error and
failed to compile (E0308, cargo101, driver1); it was not counted as discrimination.
The probe was corrected to inject Err(undefined) in the native primitive fixture
and the entire isolated sequence reran. Both production and fixture bytes were
restored to the implementation commit, with src/tests diff exit0.

Read-only sub-agent review found the initial lazy/cancelled futures and bypassed
string methods; both were corrected before controls. It also found the missing
source rejection tag and callback/resolver identity witnesses; those were fixed
and the rejection gap was explicitly discriminated by control9. Reviewers ran
no gates. Earlier development runs exposed fixture-only unexpected-post error
text mismatch, corrected before the committed control base.

## Merge preservation and restored gates

Whole crate src/tests diff from the control base exited1: integration added
core_account_snapshots and its export/tests. Both export groups were retained.
Shared exchange/transport and Cargo/package inputs changed, so earlier broad
compile-input preservation is not claimed. All three own production modules,
three test/oracle/support files, five TS source inputs and two own manifests
diffed byte-unchanged, exit0. Positive gates below were measured after this merge.

At the restored gate SHA:

| Gate | Exit / inspected output |
|---|---|
| Node24.20.0 scoped rest_helpers target | 0; nine passed, zero failed |
| Node26.7.0 same target | 0; nine passed, zero failed |
| Pinned Yarn parity:extract:check | 0; Would update 0 manifests; 0 added, 0 orphaned symbols |

Scoped command: `cargo test --offline --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --test rest_helpers --jobs 2`. Node26 uses
`PATH=/opt/homebrew/opt/node@26/bin:$PATH`; both use the explicit TypeScript path.
`cargo check --manifest-path sdk-rs/Cargo.toml
-p safe-cex-venue-hyperliquid --all-targets --jobs 2` exited0 at the same SHA,
with `Finished dev profile` inspected. This is scoped host compilation; no
workspace or wasm/clippy compilation claim.

The pinned Yarn manifest writer ran after edits: exit0, `Updated 2 manifests;
0 added, 0 orphaned symbols.` Yarn output text was inspected separately from
exit status. Cargo operations ran sequentially.

Not run: workspace/full-crate/monorepo suites, host clippy, wasm check/clippy,
Rust inventory/provenance gates, CI reruns, live exchange/browser tests.

## Push merge preservation

Final fetch merged integration `98a847bb171d9a895a95f1e4e287c9126fc2c66e` at
`a140e034486da56c90cb76c013ae6a7f5a70b429`. The whole venue src/tests, Cargo
lock/workspace/crate files, five source inputs, two own manifests, package.json
and yarn.lock diff from the restored gate SHA exited0. Thus the measured scoped
cargo and Node24/26 inputs remain byte-unchanged.

A conservative Cargo.lock walk found481 dependency packages and eight local
packages, ending when the worklist emptied after539 pops. No feature-pruning
claim. Package structures matched the restored gate SHA; all local src/tests,
Cargo.toml and build.rs inputs diffed byte-unchanged, exit0. Local packages:
safe-cex-candle-cache, safe-cex-crypto, safe-cex-exchange, safe-cex-num,
safe-cex-store, safe-cex-transport, safe-cex-types, safe-cex-venue-hyperliquid.

Other venues' manifests changed. The pinned Yarn extractor check reran at this
final merge and exited0: `Would update 0 manifests; 0 added, 0 orphaned symbols.`
Owned transitions/census remain eight pending→ported and `(127,0,34)` in the
nine assigned files; whole Hyperliquid remains `(344,106,81)`.
The evidence-only successor changes none of these measured inputs.
