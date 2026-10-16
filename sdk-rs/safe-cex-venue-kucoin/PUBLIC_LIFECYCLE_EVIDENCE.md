# KuCoin public-open, market additions and ticker lookup parity

Four rows moved pending -> ported; zero divergent rows added. This slice moves
eight rows with OKX. Independent Python 3.11 tomllib comparison covered all 12
KuCoin manifests, stopping at `kucoinspot.ws-public.ts.toml`: parent 76 ported /
39 divergent / 76 pending; measured merge head 80 / 39 / 72. Exactly four
pending -> ported changes and no other status changes; census is not proof.

Branch `feat/okx-kucoin-public-lifecycle` was created directly from integration
`7fd5102999754d0a1d95910af1fb540c3bffb94e`, with no inherited old feature branch.
Source/control SHA: `8ffc43fee2cfb3bb92705f7055acfac2f7ff97dc`.
Integration `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444` merged at measured gate
SHA `b014dc84c0c38e37b071a5162f9da3f2ab648703`.

Mapped functions in `src/public_lifecycle.rs`: futures `onOpen` ->
`on_open_futures`, spot `onOpen` -> `on_open_spot`, futures `#onMarketsChanged`
-> `on_markets_changed`, futures `#tickerForApiSymbol` -> `ticker_for_api_symbol`.
Owning websocket classes, core sender/background effects, and store assembly
remain pending. Host actions own complete live member/callback expressions;
the lookup callback owns table acquisition before toInternalSymbol, inherited
table semantics, and the current keys.

Market additions preserve empty/ws/disposed/connected/role guard order, shard
set construction before map-length capture, current HasProperty/Get access,
truthy/owned filtering with duplicates/order retained, and one channel operation.
MarketAdditions host methods retain inherited indices, index getters, length
coercion and original errors. None from `at` means HasProperty=false; a present
undefined element must be represented by a native M and handled by the field
callback. No borrow is held across callbacks. Added-list/field/role/disposal
mutations remain visible at the same source points, without later guard rechecks.

## Actual measured gates

Commands ran sequentially at `b014dc84c0c38e37b071a5162f9da3f2ab648703`, with
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 0 |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test public_lifecycle` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 |

Extractor output reported 7341 symbols / 512 files and zero updates/additions/
orphans. Writer runs followed manifest edits and also reported zero updates.
The explicit venue oracle now includes `--test public_lifecycle`; the whole venue
oracle command was not run.

Native assertions precede all live-source comparisons. Declared floor: 137 cases
= 48 futures opens + 48 spot opens + 16 lookups + 25 market-change scenarios.
Open fixtures use independent action lists, early disposal, late flag changes
and failure sites; they assert full errors and Rc identity. Lookup fixtures
check absence/empty guards, original raw UTF16 symbol units, count, actual pointee,
object/error identity. Market fixtures assert literal event order, owned symbols,
channel/operation, duplicate retention and independent send counts. Extra paired
cases cover inherited index 6, index getter changing a later element, index getter
error, and proxy length error. Earlier cases cover shard-time append/clear and
map-time append/delete/replace, keeping the correct map-length snapshot.

Enumeration stopped at disposed / enabled / last-action flip / reconnect-emitter
failure for each open kind; surrogate symbol / present / lookup error; proxy
length error for market changes. These floors are not complete input enumeration,
concrete class integration, or network subscription proof.

## Compiling discrimination

At `8ffc43fee2cfb3bb92705f7055acfac2f7ff97dc`, each compiling mutation exited 101
at the named native assertion; finally restored the original bytes; the exact
same focused test command then exited 0:

| Mutation | Native assertion |
| --- | --- |
| Futures subscribe action -> unsubscribe | `OPEN native call order kucoin` |
| Spot ping action -> pong | `OPEN native call order kucoinspot` |
| Invert owned-shard membership filter | `KU-MARKETS native owned symbols/order/duplicates` |
| Invert nonempty ticker-symbol predicate | `KU-PUBLIC native lazy lookup count` |

Zero survivors among these four controls. No untried controls are implied.
Post-merge exact src/tests/scripts/manifests/package comparison against
`8ffc43fee2` exited 0. Fresh gates above were measured at the actual merge SHA;
controls retain their original source SHA. The single shared exemption conflict
was resolved as an exact union retaining our five items and 39 incoming entries.

Sub-agent static review verified live inherited/getter array access, Rc wording,
exact target notes and all five support-item exemptions; no remaining blocker,
no tests run by reviewer. Not run: workspace build, full venue suite, host/wasm
clippy, full Rust-index check, CI, network frames or concrete websocket classes.

## Delivery merge verification

Integration `4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a` was merged at
`a4abad99049c7b1b9e89c7741143fbecf392820c`. The only conflict was the shared
exemption list: 544 ours / 556 theirs / 17 incoming / 561 union. Resolution
verified identical common entries and reasons, exact union, and no duplicates.

At this merge SHA, scoped `git diff --exit-code` against measured gate SHA
`b014dc84c0c38e37b071a5162f9da3f2ab648703` exited 0 for both venues' src,
tests, scripts, manifests and package.json. Independent changed-path inspection
found only other venue files, parity metadata and these receipts; no shared
crate source, Cargo input, TypeScript oracle source or loader changed. The four
cargo gate exits remain measured at b014dc84c0; they were not rerun or relabeled
as measurements at a4abad9904.

The changed metadata gate was rerun at a4abad9904:
`node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`
exited 0. Output: 7341 symbols across 512 source files; would update 0 manifests,
0 added and 0 orphaned symbols. Independent tomllib census at this same SHA
again covered all 12 manifests per venue, stopped at the files named above,
and found exactly four pending -> ported changes per venue against the actual
7fd5102999 parent, with no other status changes. Counts remain OKX 63 / 19 / 81
and KuCoin 80 / 39 / 72 (ported / divergent / pending).

## PR 7544 merge repair

Requested integration `518d08ff3e5cc6800f6f41ecbd08e02a7112ace2` merged into
this branch at `1c95a470117f4449ee8c5de0941c7c8e41e70190`. Only exempt.toml
conflicted. Exact parser-verified union retained 561 ours / 580 theirs,
24 incoming entries / 585 total, with no duplicate targets or changed reasons.

Scoped git diff --exit-code against reviewed delivery
`52fc42cd3afd1753c3b0d0a9503064732051d461` exited 0 for both venues'
src/tests/scripts/manifests and package.json. A separate scoped diff of
shared Cargo inputs, transitive shared crate source, the source loader and
both maintained TypeScript venue directories against actual cargo gate SHA
`b014dc84c0c38e37b071a5162f9da3f2ab648703` also exited 0. Previous cargo
exits therefore stand at their actual measurement SHA; no cargo was rerun.

At repair merge `1c95a470117f4449ee8c5de0941c7c8e41e70190`, changed metadata
gate `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`
exited 0: 7341 symbols / 512 files, would update 0, added 0, orphaned 0.
The original eight ported rows are preserved; this repair moves zero new rows.
