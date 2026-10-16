# Socket ping and UTA promotion ports

Moved **4 rows pending→ported, 0→divergent**:
`BitgetPublicWebsocket.ping`, `BitgetSecondaryPublicWebsocket.ping`,
`BitgetPrivateWebsocket.ping`, and
`BitgetExchange#promoteUtaRoutingFromClassicUnsupported`.

**Tested code SHA:** `965e254c8085a312e61e6d67851b16fd9d0d2d5a`.
**Actual/direct parent:** integration
`128daa572bff854c64297a24e95ac320b7f78f60`.
Branch: `feat/rust-venue-bitget-reads`; no PR 7400 commits are ancestors of this
slice. No additional unreviewed branch ancestor was added. This file is committed
separately; its later documentation SHA is not the SHA at which gates were run.

Python 3.11 `tomllib` parsed all eleven Bitget manifests at both revisions and
matched their `(file, symbol, kind)` keys. Both have 227 rows. The measured base
has **37 ported / 71 divergent / 119 pending**, not 71 pending; the tested SHA
has **41 ported / 71 divergent / 115 pending**. Exactly the four named rows change
status. PR 7400's three rows remain pending at this base and were not re-ported.
This is a manifest census, not proof of the complete venue.

## Measured gates at tested code SHA

All Cargo invocations ran sequentially with `/opt/homebrew/opt/rustup/bin` on
PATH. Source tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Measured exit / output |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | 0; finished dev profile, zero compiler errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-bitget --test socket_policy --jobs 2 -- --include-ignored` after controls restored | 0; 2 passed / 0 failed / 0 ignored |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; complete output read: would update 0 manifests, 0 added, 0 orphaned |
| `rustfmt --edition 2024 --check` on socket_ping.rs, account_predicates.rs and tests/socket_policy.rs | 0 |
| `git diff --check` and restored `git diff --exit-code` | 0 / 0 |

The extractor parse/serialize API updated mappings and file provenance, and the
writer ran before commit with zero reconciliation changes. No manifest array was
hand-spelled. Full workspace compilation/suites, full venue oracle, global Rust
manifest checker, host Clippy, wasm Clippy/build, real sockets and CI were not run.

Each new row received one compiling, on-disk discrimination control:

| Mutation | Named failed assertion; measured exit |
| --- | --- |
| Public `ping` text changed to `pong` | `PING native text public`; 101 |
| Secondary clock shifted by +1 | `PING native timestamp before send secondary`; 101 |
| Private disposed guard inverted | `PING native final timestamp private`; 101 |
| Promotion writes detected=false | `PROMOTION native detected flag`; 101 |

All four edits were restored byte-for-byte. No survivor was observed. The restored
target passes both tests at exit 0. These are one-time controls per newly ported
row, not a new exhaustive mutation matrix.

## Behavioral floor and review

The independently fixed input walk contains **59 cases**: eight per ping body,
32 promotion combinations and three predicate/getter failures. Ping cases use
two clock inputs (-0 and 103.5), disposed/send-failure flags and a previous 37.5
timestamp. Promotion covers payload absent/present, all three Boolean guards,
and complementary initial UTA/detected flag pairs. Enumeration stops after the
private ping's disposed/failing/103.5 case, then the final promotion getter error
at testnet. It does not exhaust f64 inputs, all initial flag pairs or host behavior.

All decoded native bits, state, payload/error identities and effect sequences are
asserted before any native observation is serialized. Source input identity and
59 consumed entries are independently pinned. The actual four TypeScript field
bodies execute with supplied host/predicate operations; imported predicate behavior
retains its separate existing manifest contract. Transpilation is not semantic
TypeScript validation. Concrete socket and Exchange assembly remain pending.

Sub-agent `/root/account_read_review` reviewed exact tested SHA and parent:
**no blocking findings, and no real new divergence requiring `divergent` status**
for these field bodies. It inspected committed code/tests/mappings and actual TS
bodies without re-reading CLAUDE.md or memory. Its source capture and independent
59-case checker each exited 0. It did not run Cargo, extractor, mutation controls,
Clippy, wasm, CI or full suites. The four rows are ported at their supplied
operation boundaries; no repair of the predicate's separately mapped domain or
automatic socket/Exchange registration is claimed.
