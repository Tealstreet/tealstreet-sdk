# OKX public-open, subscriptions and notional parity

Four rows moved pending -> ported; zero divergent rows added. With KuCoin,
this slice moves eight rows. Independent Python 3.11 tomllib comparison covered
all 12 OKX manifests, stopping at `okxeudemo.exchange.ts.toml`: parent 59 ported /
19 divergent / 85 pending; measured merge head 63 / 19 / 81. Exactly four
pending -> ported changes and no other status changes were found. Census is
separate from behavioral proof.

Fresh branch `feat/okx-kucoin-public-lifecycle` was based directly on integration
`7fd5102999754d0a1d95910af1fb540c3bffb94e`; no old feature branch was inherited.
Source/control commit: `8ffc43fee2cfb3bb92705f7055acfac2f7ff97dc`.
Integration `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444` was merged at measured
gate SHA `b014dc84c0c38e37b071a5162f9da3f2ab648703`.

Mapped bodies in `src/public_lifecycle.rs`: `OKXPublicWebsocket.onOpen` ->
`on_open_public`, `OKXPrivateWebsocket.subscribe` -> `subscribe_private`,
`OKXBusinessWebsocket.subscribe` -> `subscribe_business`, and
`OKXExchange#contractsToNotional` -> `contracts_to_notional`.

The open sequence has one entry disposal guard, ordered clear/subscribe/ping/
emitter actions, and a late feature read. Host actions evaluate complete live
member/callback expressions. Private subscription constructs account-first
product records; business constructs order channels, sends once before reading
the live topic snapshot, then uses 100-topic chunks. Existing topics use Rc<T>,
retaining identity and later-chunk field changes. Host capabilities own imported
Object.values/flatten and JSON.stringify/logged-send effects. Owning classes and
concrete sender/background effects remain pending; this is not a core non-OPEN
send or live network acceptance claim.

Notional conversion preserves numeric contractSize || 1, always reads contract
currency, then inverse and lazy quote-uppercase getters. It preserves full UTF16
currency units, getter errors, and `(contracts * mark) * size` association, including
overflow. Unknown-input currency coercion and owning exchange assembly remain
pending. The mutable ORDER_CHANNELS Set row was left pending rather than mapped
to a frozen array.

## Gates measured at the actual SHA

All commands below ran sequentially at
`b014dc84c0c38e37b071a5162f9da3f2ab648703`. Oracle environment:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2` | 0 |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test public_lifecycle` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 |

Extractor text: 7341 symbols across 512 source files;
`Would update 0 manifests; 0 added, 0 orphaned symbols`. Writer runs after each
manifest edit also reported zero updates/additions/orphans. Metadata arrays
and exemptions were serialized programmatically.

The declared native/source floor is 348 cases: 48 public opens, 240 subscriptions,
60 notional cases. Every native call order, argument, count, Rc identity, complete
error, numeric value or NaN-classification assertion precedes the live oracle.
NaN payload identity is not claimed; classification is checked before its wire
view. Expected scalars and callback traces are literal fixtures, not generated
from output objects. Frame counts come from input lists/counts. Subscription
witnesses include empty/duplicate instruments, 0/1/100/101/201 existing topics,
first-send replacement of the live topic list, and later-chunk field mutation.

Enumeration stopped at disposed / enabled / last-action flip / reconnect-emitter
failure for opens; business / duplicate instruments / 201 topics / send #3 failure
for subscriptions; missing contractSize / inverse / quote getter failure for
notional. These finite floors do not claim complete input enumeration.

## Compiling controls

Each control at `8ffc43fee2cfb3bb92705f7055acfac2f7ff97dc` compiled, exited 101 at
the named native assertion, restored bytes in finally, then reran the identical
focused cargo test command and exited 0:

| Production mutation | Native assertion |
| --- | --- |
| Change public sequence-clear action | `OPEN native call order okx` |
| Swap private positions/orders channels | `OKX-SUB native product channel/order` |
| Business chunks 100 -> 99 | `OKX-SUB native batch floor` |
| Ordinary notional first multiply -> addition | `NOTIONAL native scalar value` |

Zero survivors among these four controls. Preliminary precommit compilation
initially failed on an unused test import; it was fixed before source commit.
That compiler failure is not a discrimination result.

The integration merge had one exempt.toml append conflict. Its resolution retained
our five exact support-item exemptions and 39 incoming entries, comparing every
entry/reason against the union of both stage inputs and rejecting duplicates.
Exact scoped diff against `8ffc43fee2` over both venue src/tests/scripts, their
manifests and package.json exited 0 after merge; the fresh gates above were then
measured at the merge SHA. The earlier control exits stay bound to their own SHA.

Sub-agent static reviews found and verified fixes for Rc carrier wording and
live inherited/getter array access, then reported no remaining static blocker;
reviewer ran no gates. Not run in this slice: workspace compilation, full venue
suites, host clippy, wasm clippy, full Rust-index check, CI, or live network tests.

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
