# KuCoin Spot operational bodies

Branch: feat/kucoin-spot-account-order-bodies.
Actual fresh integration parent: 076f1ae3492a5c5acf63b637fea6da5cd008cb5f.
Source/test gate SHA: 26069d652c4aeaa8296ed453679fa555d1cccf01.
No unreviewed feature ancestor was inherited.

Independent Python 3.11 tomllib comparison at source commit
9fd79a53543b8c452cf5e52e8b47537c4e9ca431 measured **8 pending -> ported,
0 -> divergent** against the actual parent: 130 ported / 39 divergent / 22 pending
became 138 / 39 / 14. The later test-only correction does not change manifests.
The 12-manifest census stopped at kucoinspot.ws-public.ts.toml; it is a census,
not proof of other rows. This slice ports KucoinSpotExchange.start, tick,
fetchBalance, fetchOrders, placeOrder, updateOrder, fetchHistoricalFills and
fetchOHLCV. Constructor and complete exchange/provider assembly stay pending.

The official manifest parser/serializer generated root/transitive targets and
provenance. Independent read-only review checked all 27 actual functions and
exact direct-call closures; function enumeration stops at map_candle. All
algorithms are mapped. Only SpotBodyOps has a supporting C3 record, beside
existing KuCoin declarations. Parsed exemption comparison with the fresh parent
preserved all 1,165 incoming records and their relative order, plus precisely
one new record (1,166 total). The writer exited 0: zero updated manifests,
additions and orphans. Root package JSON differs only by appending
` --test spot_bodies` to venue-oracle:kucoin. The full venue command was not run.

## Bodies and mandatory capability contracts

Native functions own the eight source bodies' branches, payload construction,
callbacks, pagination, pricing accumulation, sorting, await/try boundaries and
original returns/errors. Start has no new catch. Tick initiates all three reads
under a captured Promise.all, binds the original values, retains inner disposal
return and loaded spreads, then schedules a live tick callback returning its
original handle. Balance retains trade query, strict success guard, original
fallback/info/account handles, native totals and iterator closing on body errors.

Balance's two ordinary computed assignments capture the target/property key
before RHS evaluation and then perform strict Set, preserving prototype/setter
behavior. They are not object-literal DefineDataProperty. Open orders retain
native per-symbol paging with 50-item requests and a 100-page cap, mapper/argument
spread, active-symbol discovery, Promise.all/flat and original store fallback.
Placement/amendment retain pre-request validation, caller ID, inherited-property
in semantics under the host contract (fixtures cover own update fields),
undefined/nullish distinctions, adjust/omitUndefined/UUID imports, unlimited POST,
optional returned ID/fallback, complete fresh errors and original catch/rethrow.
Fills retain options/limit/timestamp/side/fee behavior; candles retain dayjs bounds,
floor-second queries, success guard, exact native fields and ascending timestamp
comparator returning the same array. Candle transport rejection propagates.

Source behavior preserved: with missing totalPage, the finite total guard leaves
totalPage at 1. A full first page sets hasMorePages true and increments pageNum,
but the next while guard is 2 <= 1, so it stops. The native/source witness returns
50 rows from one request. This is a finite source behavior finding, not evidence
about live exchange responses, and no TypeScript repair is included.

SpotBodyOps extends existing venue-local BootstrapOps. Required bindings implement
complete live member/callee/property and language/intrinsic/import operations,
computed references and ordinary assignment, constructor behavior, argument
spread, iterable triple/interval binding and iterator closing with an existing
throw. Async mapper hosts poll new futures eagerly and retain initiated,
noncancelling host work. No shared type/trait or core query ABI changed.
Concrete exchange/provider/language assembly, arbitrary thenables/scheduling and
exotic host objects are not proven by these finite fixture implementations.

## Measured gates

At 26069d652c4aeaa8296ed453679fa555d1cccf01, sequential commands measured:

| Command | Exit | Observed output |
| --- | --- | --- |
| /opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 | Finished dev profile, no errors |
| /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --test spot_bodies --jobs 2 | 0 | spot_bodies_native_before_source_oracle; 1 passed, 0 failed |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | 7,341 symbols / 512 files; zero updates/additions/orphans |

The focused target used
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
Extractor output text was inspected, not inferred solely from the process exit.
Raw records/logs: /tmp/kucoin-spot-bodies-source-gates.json and adjacent logs.
Earlier gates at 9fd79a53543b8c452cf5e52e8b47537c4e9ca431 also exited 0, but
are not the proof for the later native expectation correction. Development
compile failures (radix/Debug omissions) and an incorrect initial independent
missing-total expectation were corrected; none is counted as a mutation gate.

## Native/source proof floor

There are 55 independently listed fixtures: start 8, tick 5, balance 6,
orders 9, placement 10, amendment 6, fills 5 and candles 6. Enumeration stops
at ohlcv/reject. Pagination enumeration stops at request 100, for one symbol;
multiple-symbol/custom-iterator async interleavings are outside the fixture floor.
All native assertions for all fixtures run before packing any native result or
invoking the maintained-source comparison. Native assertions require actual
number/string/Boolean variants and literal values/bit patterns, independent
rejection/emission/row/request counts, route/query bounds, original arrays,
account/info/mapper-row/caller/error handles and scheduled callback return handles.
Every diagnostic string and full original/fresh Error metadata is checked natively.

Expected rejection and emission counts come from literal method/case expectations,
never from result.is_err() or output lengths. The prototype fixture requires two
native target/prototype links, original account info, decoded fields, and exactly
one own USDT entry on each target; the actual source independently checks
Object.getPrototypeOf before packing. Body failure closes once with original
fallback, while guarded error paths retain their source diagnostics.

The oracle extracts the unchanged eight source initializers, actual endpoint/
interval/page constants, enums, adjust and omitUndefined helpers. It uses installed
lodash omitBy and actual dayjs with a fixed Date. Arrow factories preserve receiver
lexical this. Other receiver methods (including mapOrder and getUsdPrice), store,
XHR, UUID and clocks are controlled finite bindings. Ordinary dense arrays/plain
records, fixed numeric inputs and the prototype/iterator failure witnesses are
the tested floor. This is not full class/provider/network integration.

## Review and unrun scopes

Eight production mutations measured at 26069d652c4aeaa8296ed453679fa555d1cccf01
each compiled, failed the named native assertion at exit 101 before any wire
comparison, restored exact source bytes, then passed the focused target at 0.

| Injected change | Named native failure | Broken / restored exits |
| --- | --- | --- |
| Loaded flags false | START native markets loaded | 101 / 0 |
| Return before scheduling the tick loop | TICK native loop floor | 101 / 0 |
| Use holdings instead of available balance in free total | BALANCE native aggregate | 101 / 0 |
| Reduce maximum open-order pages to 99 | ORDERS native independent output count | 101 / 0 |
| Swallow caught placement error and return PLACED | SPOT native independent rejection: place/bad | 101 / 0 |
| Return WRONG instead of original amendment ID | UPDATE native replacement fallback | 101 / 0 |
| Decode fill quantity from price | FILLS native decoded number | 101 / 0 |
| Reverse candle timestamp comparator | OHLCV native ordered decoded candle | 101 / 0 |

No control survived. This eight-control enumeration stopped at candle-sort;
it is not exhaustive. No compiler failure was counted as negative proof.
Raw records/logs: /tmp/kucoin-spot-bodies-controls.json and adjacent logs.
The swallowed-error mutation specifically discriminates the corrected literal
rejection expectation; a success-shaped PLACED value cannot satisfy it.

Read-only sub-agent review found the computed assignment distinction and an
outcome-derived native expectation; both were corrected. Exact mappings, original
handle/prototype witnesses and maintained oracle wiring were reviewed. Reviewer
ran no cargo, extractor or oracle gates.

Not run: workspace compilation, full KuCoin suite/maintained venue command,
other venues' tests, full monorepo suites, host clippy, wasm clippy, full Rust index,
CI or live exchange/API/network tests. No claim of complete remaining venue
coverage, assembled class/provider behavior or arbitrary host timing equivalence.

## Integration merges and refreshed measurements

Incoming 7600c3e6ecfb331029421876f9d41adc40813c2d merged at
a56569f06c2401b5c4077f313b02d7c010d35fcc. Scoped committed and working-tree
git diff --exit-code comparisons with 26069d652c4aeaa8296ed453679fa555d1cccf01
exited 0 for KuCoin source/tests/scripts/Cargo.toml, its parity manifests and
root package. Six dependency crates and maintained TypeScript/helpers/yarn.lock
were also byte-unchanged (exit 0). SDK Cargo.toml/Cargo.lock changed, so check,
focused test and extractor were measured afresh at this merge: each exit 0;
observed finished dev profile, 1 passed/0 failed and zero extractor drift.
Raw records: /tmp/kucoin-spot-bodies-merge-gates.json and adjacent logs.

Integration advanced with shared exchange/transport changes. Incoming
a07e971bb08f867925ff18ca8b13d2f288d0deba merged at
e3a64f6a1c3a75b94951d91ae4afed56641642ed. The same owned-source/tests/scripts/
crate-manifest/KuCoin-manifests/package committed AND working-tree comparisons
against the source gate SHA again exited 0. Shared dependency inputs changed,
so all three exact commands above ran sequentially again at this second merge:

| Gate at e3a64f6a1c3a75b94951d91ae4afed56641642ed | Exit | Observed output |
| --- | --- | --- |
| Scoped KuCoin all-targets cargo check | 0 | Finished dev profile; no errors |
| Focused spot_bodies test with the same TypeScript path | 0 | Named native-before-source test; 1 passed, 0 failed |
| Source extractor check | 0 | 7,341 symbols / 512 files; 0 updates/additions/orphans |

Raw records: /tmp/kucoin-spot-bodies-dependency-gates.json and adjacent logs.
The receipt-only wording edit is outside these build/test/extractor inputs.
Controls remain measured at their source SHA; they were not rerun or relabeled.

Fresh independent tomllib census against the second incoming SHA again measured
exactly 8 pending -> ported, 0 -> divergent, 130/39/22 -> 138/39/14. It stopped
at kucoinspot.ws-public.ts.toml after 12 files. Parsed exemption union retained
all 1,242 incoming records, complete values and relative order, plus exactly
the source-identical SpotBodyOps record (1,243 total). Independent package JSON
comparison preserved every incoming field and added only the single oracle flag.
Sole merge base is the second incoming SHA; branch diff has eight files,
stopping at tests/spot_bodies.rs. Raw census: /tmp/kucoin-spot-bodies-dependency-census.json.

Read-only review confirmed all eight compiling named control failures/restored
passes, first-merge survival and exact row/exemption/package preservation. A
receipt phrase was corrected to distinguish required inherited-property semantics
from the own-field update fixtures; no inherited-update fixture proof is claimed.
Reviewer ran no gates. Unrun scopes and provider limits above still apply.

The push-time fetch advanced to 7d73334d51aab93a006d732997ec203cec61ffbc;
it merged at f47b04a84c099bacdb9aa7dbb51b9fa114f151f6. Only the parity spec
and a Bitrue manifest changed since the preceding merge. Scoped comparison
against e3a64f6a1c3a75b94951d91ae4afed56641642ed exited 0 for all owned inputs,
SDK Cargo.toml/Cargo.lock, all six dependency crates, exemption records, maintained
KuCoin TypeScript/types/helpers, root package and yarn.lock. Cargo gates were not
rerun at this third merge; measured input preservation carries their prior exits.
Extractor was refreshed at f47b04a84c099bacdb9aa7dbb51b9fa114f151f6: exit 0,
observed 7,341 symbols / 512 files and zero updates/additions/orphans. Raw record:
/tmp/kucoin-spot-bodies-parity-refresh.json. Fresh tomllib/package/exemption checks
again measured exactly the same eight moves and 1,242 + 1 record union, stopping
at kucoinspot.ws-public.ts.toml after 12 files. Sole merge base is the push-time
incoming SHA. Raw census: /tmp/kucoin-spot-bodies-push-census.json.
