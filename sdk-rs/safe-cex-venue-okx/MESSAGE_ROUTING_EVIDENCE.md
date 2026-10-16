# OKX message routing evidence

This slice moves 3 OKX and 5 KuCoin rows pending -> ported; zero divergent changes.
The fresh branch is `feat/okx-kucoin-message-routing`, based directly on integration
`c390d6df02c16810ac5b813f11306c858dd2704e`; no unreviewed feature branch was used as its parent.

## Mappings and scope

All three OKX rows map to real public functions in `src/message_routing.rs`:
`OKXPrivateWebsocket.onMessage` -> `private_message`,
`OKXBusinessWebsocket.onMessage` -> `business_message`, and
`OKXBusinessWebsocket#dispatchMessageHandler` -> `dispatch_message_handler`.
Each adapted manifest note names its full Rust target and capability contract.
Owning websocket classes, parser/runtime implementation and concrete transport assembly
remain outside this slice. Effects preserve synchronous invocation; ignored promises
are not awaited, matching the source invocation boundary.

Private routing retains substring precedence, opaque parsed primitive roots, and the
source's login/error catch scopes. Business routing retains repeated live arg reads,
strict Set membership, inherited timeframe lookup, lazy template conversion, and the
original uncoerced channel for dispatch. Dispatch captures its collection before stats;
the capability owns whole JavaScript for-of semantics, including IteratorClose.
This is algorithm parity at documented capability boundaries, not a complete JS runtime.

## Measured gates

Source/gate SHA: `455d66dafb169f346dd46dd4d5abbb4a7c14ae57`.
The following completed with exit 0 at that SHA:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2`
- `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test message_routing`
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`

The test used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Extractor output, not just shell status, reported 7,341 symbols / 512 files,
zero manifests to update, zero added and zero orphaned symbols.
The manifest writer `parity:extract` also completed with exit 0 after edits.
KuCoin-only cleanup SHA `daa6c442bb603ef83e913644204716c297a8ebd7`
does not relabel these measured exits; OKX source, tests and oracle were byte-preserved.

## Discrimination and fixture floor

Native assertions run before the source oracle: decoded payload and Rc identity,
raw value variants, full error fields/identity, literal traces, and UTF-16 keys.
The independent fixture expectation is 84 = 72 routing + 12 dispatch fixtures.
Routing enumeration stops after the second router's disposed-flip case; dispatch
enumeration stops at the numeric handler case. This is a finite floor, not exhaustive
coverage of arbitrary prototypes, getters, coercions or iterator implementations.
Witnesses include a second channel getter returning number 5, inherited timeframe keys,
lazy poison conversion, lone surrogates, parsed false with a Boolean prototype code
getter, captured arrays replaced during stats, and live append/truncate behavior.

Three compiling family controls at source SHA `455d66dafb169f346dd46dd4d5abbb4a7c14ae57`:

| Function / mutation | Named native failure | Broken / restored exits |
| --- | --- | --- |
| `private_message`: propagate login failure | `PRIVATE native swallowed login/error failure` | 101 / 0 |
| `business_message`: dispatch id instead of channel | `BUSINESS native original uncoerced channel` | 101 / 0 |
| `dispatch_message_handler`: fanout before stats | `DISPATCH native capture/guard/stats/live fanout order` | 101 / 0 |

Each mutation compiled, failed that assertion, was byte-restored, and passed the same
focused target. Together with the two KuCoin controls this is five family controls,
not eight individual symbol controls. Zero survivors among these five mutations.
Initial ambiguous generic error type and incorrect fixture expectation were corrected
before the measured SHA; they are not counted as mutation evidence.

## Independent census and unrun gates

Python 3.11 tomllib compared all 12 OKX manifests against the actual parent, stopping
at `okxeudemo.exchange.ts.toml`: 68/19/76 ported/divergent/pending -> 71/19/73.
The three named rows above are the only OKX status changes.
Static sub-agent review checked live reads, catch boundaries, native-first assertions,
all eight mappings and the cleanup; its review is not an executed test gate.
Workspace compilation, full venue suites, host clippy, wasm clippy, full Rust-index,
CI and exchange/network tests were not run. No completeness claim for owning classes
or concrete parser/collection runtime assembly is made.

## Integration merge and refreshed delivery gates

Merged fetched integration `c1b0ce61f2247f5604b26ed5a00b142be75d90ef` without
conflicts at `7dbc14d5816e13702954745172ea1a3a4f8ad166`.
`git diff --exit-code daa6c442bb603ef83e913644204716c297a8ebd7 HEAD --`
both venue `src`, `tests`, `scripts`, both venue manifest directories and `package.json`
completed with exit 0 at that merge. A separate OKX `src/tests/scripts` comparison
against initial gate SHA `455d66dafb169f346dd46dd4d5abbb4a7c14ae57` also exited 0.

Shared transport implementation and Cargo.lock changed. Therefore both exact scoped
checks and both exact focused `message_routing` tests listed in these two receipts
were rerun sequentially at merge SHA `7dbc14d5816e13702954745172ea1a3a4f8ad166`:
all four completed with exit 0, using the same TypeScript path for both tests.
The extractor check at that same SHA completed with exit 0 and output confirmed
7,341 symbols / 512 files, zero updates, zero added and zero orphaned symbols.
The five compiling mutation controls remain measurements at their original source SHA;
they were not rerun or relabeled as merge-SHA mutation controls.

A fresh independent tomllib census against the merged integration head, enumerating
12 manifests per venue to the same named stop files, confirmed exactly 3 OKX + 5
KuCoin pending -> ported changes, no other status edits, zero divergent changes.
Remaining: 73 OKX and 64 KuCoin pending rows. Static merge-preservation review was clear;
the reviewer ran no cargo/tests/index. The subsequent receipt-only commit changes no
gate inputs. Unrun gates and assembly limits above still apply.
