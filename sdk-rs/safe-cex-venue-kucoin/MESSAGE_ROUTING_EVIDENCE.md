# KuCoin message routing evidence

This slice moves 5 KuCoin and 3 OKX rows pending -> ported; zero divergent changes.
Branch: `feat/okx-kucoin-message-routing`.
Actual fresh parent: integration `c390d6df02c16810ac5b813f11306c858dd2704e`,
not an unreviewed feature branch.

## Mappings and scope

Five real public functions in `src/message_routing.rs` implement the adapted rows:

- `KucoinPublicWebsocket.onMessage` -> `futures_public_message`
- `KucoinPrivateWebsocket.onMessage` -> `futures_private_message`
- `KucoinSpotPublicWebsocket.onMessage` -> `spot_public_message`
- `KucoinSpotPrivateWebsocket.onMessage` -> `spot_private_message`
- `KucoinPrivateWebsocket.handleAlgoOrdersTopic` -> `algo_order_message`

Every manifest note names its full Rust target and capability contract.
Routers preserve entry-only disposal, effect order, live topic predicates, raw split
components and mapping fallback, lazy template conversion, statistics and callback
invocation. The four routing functions form one family. Whole expression capabilities
retain JavaScript getter/split/coercion and for-of or forEach behavior; concrete owning
classes, parser, stores and callback runtime implementations remain outside this slice.
Effects preserve synchronous invocation and source-ignored promises, not awaited results.
Algo handling reads the captured event after map/first and before the parent guard;
native `OrderStatus::Canceled` is asserted before source/wire comparison. The oracle
loads the maintained TypeScript enum and jsonParse source rather than substitute values.

## Measured gates

Source and mutation SHA: `455d66dafb169f346dd46dd4d5abbb4a7c14ae57`.
The following completed with exit 0 at that SHA:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2`
- `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test message_routing`
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`

After inlining the single remaining book-key helper and removing its dead interval
branch, the same scoped KuCoin check and focused test completed with exit 0 at cleanup
SHA `daa6c442bb603ef83e913644204716c297a8ebd7`.
The test env was `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Extractor output at the source SHA reported 7,341 symbols / 512 files, zero updates,
zero added and zero orphaned symbols; the writer also completed with exit 0 after edits.
Both routing modules now have exactly eight public functions total, no additional
helper/type/trait and no new supporting exemptions.

## Discrimination and fixture floor

All decoded native value, Rc identity, full error and independent literal trace/key
assertions precede the source child. Fixture floor: 142 = 128 routers + 14 algo cases.
The four-router enumeration stops after spot-private disposed-flip; algo enumeration
stops at case 13, store error. Live getter counts and array visits use literal expected
values. A custom topic split returning number 7 is asserted as a native numeric component
before mapping and becomes `orderBook_7` only at template conversion. Source assertions
also check its numeric type. Append visits differ for for-of versus forEach initial length.
This floor does not enumerate every possible prototype, iterator or conversion hook.

Two compiling family controls at `455d66dafb169f346dd46dd4d5abbb4a7c14ae57`:

| Function / mutation | Named native failure | Broken / restored exits |
| --- | --- | --- |
| `futures_public_message`: ticker statistics -> wrong | `ROUTE native selected effects/key/order` | 101 / 0 |
| `algo_order_message`: Canceled -> Closed | `ALGO native canceled enum` | 101 / 0 |

Each mutation compiled, failed the named assertion, was byte-restored and passed the
same focused target. The other three router symbols were exercised in the 128-router
fixture floor but were not individually mutated. With the three OKX controls this is
five family controls for eight rows, zero survivors; it is not eight individual controls.

## Independent census and unrun gates

Python 3.11 tomllib compared all 12 KuCoin manifests against the actual parent, stopping
at `kucoinspot.ws-public.ts.toml`: 83/39/69 ported/divergent/pending -> 88/39/64.
Only the five named rows change status. Static sub-agent review found a too-narrow string
split boundary; the production interface was widened and paired numeric witness added
before the measured SHA. Cleanup was reviewed as semantically identical. Review is not
an executed gate. Workspace compilation, full venue suites, host clippy, wasm clippy,
full Rust-index, CI and exchange/network tests were not run. Concrete class/runtime
assembly remains pending; no census or test floor is claimed as exhaustive parity proof.

## Integration merge and refreshed delivery gates

Merged fetched integration `c1b0ce61f2247f5604b26ed5a00b142be75d90ef` without
conflicts at `7dbc14d5816e13702954745172ea1a3a4f8ad166`.
`git diff --exit-code daa6c442bb603ef83e913644204716c297a8ebd7 HEAD --`
both venue `src`, `tests`, `scripts`, both venue manifest directories and `package.json`
completed with exit 0 at that merge. A separate OKX `src/tests/scripts` comparison
against initial gate SHA `455d66dafb169f346dd46dd4d5abbb4a7c14ae57` also exited 0.

Shared transport implementation and Cargo.lock changed. Both exact scoped checks and
both exact focused `message_routing` tests listed in these two receipts were rerun
sequentially at `7dbc14d5816e13702954745172ea1a3a4f8ad166`: all four completed
with exit 0, using the same TypeScript path for both tests. The extractor check at that
same SHA completed with exit 0; output confirmed 7,341 symbols / 512 files, zero updates,
zero added and zero orphaned symbols. The five compiling mutation controls remain
measurements at their original source SHA, not merge-SHA mutation controls.

A fresh independent tomllib census against the merged integration head, enumerating
12 manifests per venue to the same named stop files, confirmed exactly 3 OKX + 5
KuCoin pending -> ported changes, no other status edits, zero divergent changes.
Remaining: 73 OKX and 64 KuCoin pending rows. Static merge-preservation review was clear;
the reviewer ran no cargo/tests/index. The subsequent receipt-only commit changes no
gate inputs. Unrun gates and assembly limits above still apply.
