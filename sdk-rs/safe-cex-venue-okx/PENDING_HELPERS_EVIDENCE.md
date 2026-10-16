# Pending helper slice

Moved **4 pending to ported, 4 pending to divergent** across both venues.
OKX contributes four price-load membership methods (ported) and parseFinite /
sumFinite (CHOSEN divergent). KuCoin contributes the two guarded Spot topic
operations (FORCED divergent). Python 3.11 tomllib parsed all twelve manifests
per venue at actual parent `f436fc816390a8b4556ab7a2fe0ae85072337036` and gate
SHA `3e315f89dd4216ff959fb958bb8aad02f4b8b66d`:

| Venue | Parent ported/divergent/pending | Gate ported/divergent/pending |
| --- | --- | --- |
| OKX | 46 / 16 / 101 | 50 / 18 / 95 |
| KuCoin | 67 / 37 / 87 | 67 / 39 / 85 |

Fresh branch `feat/okx-kucoin-pending-helpers` started exactly at f436fc8163.
No unreviewed feature ancestor was adopted. Gate commit's actual parent is
`a2375d871bfee86fdd9548ac342a1a47f390c65e`, the reviewed topic implementation;
fixture adaptation and finite implementation precede it on this same slice.
These exits cover the named gate SHA, not a guessed future delivery SHA.

## Measured gates

Only one cargo ran at a time, with --jobs 2. Tests set
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
All following exits were measured at 3e315f89dd:

| Command | Exit and scope |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2` | 0, zero errors |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 0, zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test finite --test price_load --test logged_ping --test ping -- --include-ignored` | 0, four focused tests passed |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test ws_helpers ping::` | 0, two focused tests passed; seven filtered out |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; output inspected: 7341 symbols / 512 sources, zero changes/additions/orphans |
| `rustfmt --edition 2024 --check` for price_load source/test, finite test and KuCoin shared websocket test fixture | 0 |

Manifest edits used extractor parseManifest / serializeManifest. Each writer
run used parity:extract and exited 0 with inspected zero updates/additions/
orphans. No manifest array was hand-spelled. Existing oracle discovery includes
these tests without package script changes.

## Native evidence and boundaries

Price-load add/has methods use caller-owned separate sets and exact UTF16
code-unit keys. Five independently declared IDs include ASCII, empty, D800,
DC00 and a paired emoji. Native insert identities, separate membership,
duplicate counts and forty query booleans precede source captures. The floor
stops at the paired emoji after index insertion. Full exchange constructor,
class state ownership and observable Set enumeration remain pending.

Finite helpers assert thirty-five native value/error profiles before any
source comparison. Literal f64 expectations pin bits including negative zero,
left-to-right addition and overflow. The floor stops at the sum coercion-error
case. CHOSEN: the accepted surface is JSON wire data. A live object whose
callable toString returns 7.5 gives 7.5 in both source methods; its JSON view is
{}, for which native methods return 0. Executable object hooks, Symbols and
sparse/cyclic containers are excluded: adding a second executable object
runtime to a venue wire parser is the declined alternative. No claim of full
unknown-value parity is made.

Topic tests have forty-eight declared native profiles, twelve independently
expected frames, and complete errors including subscriber IDs/code/context.
Date-before-send effects and every decoded field are asserted before any live
wire bytes. The floor stops at unsubscribe / Unicode topic / connected /
disposed / send failure. FORCED: the required &str / JsJsonValue::String topic
surface cannot hold materialized D800. Each actual source method transmits it;
source JSON.parse retains length one and unit 55296. Native String decoding
refuses the entire topic before it can be passed to the sender. Literal ASCII
backslash-u remains a distinct positive. No surrogate substitution is used.

## Review and compile-floor repair

Static sub-agent review has no outstanding findings; the reviewer ran no
cargo/tests. It found the original over-broad ported classifications and a
readiness assertion backed by no-op close/terminate methods. Buckets/witnesses
are now explicit, and close/terminate set ready state 3.

The first uncommitted KuCoin focused test exited 101 with E0407/E0046: landed
core moved the monotonic clock from WsRuntime to ExchangeRuntime. Websocket
fixtures now retain their clock traces under monotonic_now_ms. Eleven ordinary
venue fixtures delegate that required method to their deterministic now_ms.
All-target checks compile those fixtures; their other test targets were not
executed. Focused tests cover the changed websocket clock behavior.

Not run: workspace compilation, full venue/monorepo suites, Rust parity suite,
host Clippy, wasm build/Clippy, or CI. Previously ported rows received no new
mutation matrices. Controls for newly moved rows and merge-down proof follow.

## Discrimination controls at 3e315f89dd

Each production mutation compiled, the named native assertion failed with
exit 101, finally restored the file byte-for-byte, and the same exact focused
test passed with exit 0:

| New row | Compiling mutation | Named failed assertion | Broken / restored |
| --- | --- | --- | --- |
| parseFinite | invert is_finite branch | `OKX-FINITE native parse 0` | 101 / 0 |
| sumFinite | subtract each parsed item | `OKX-FINITE native sum 2` | 101 / 0 |
| subscribeTopic | send unsubscribe operation | `KUCOIN-TOPIC native operation subscribeTopic` | 101 / 0 |
| unsubscribeTopic | send subscribe operation | `KUCOIN-TOPIC native operation unsubscribeTopic` | 101 / 0 |
| markPriceLoaded | insert empty key instead of supplied key | `OKX-LOAD native mark insertion 0` | 101 / 0 |
| indexPriceLoaded | insert empty key instead of supplied key | `OKX-LOAD native index insertion 0` | 101 / 0 |
| hasMarkPriceLoaded | negate membership | `OKX-LOAD native unseen queries 0` | 101 / 0 |
| hasIndexPriceLoaded | negate membership | `OKX-LOAD native unseen queries 0` | 101 / 0 |

Tests were finite_helpers_native_values_before_source (finite target),
price_load_native_utf16_state_before_source (price_load target), and
ping::spot_topics_native_values_before_wire (ws_helpers target), each filtered
with --exact. None survived. An earlier mark insertion -> clear mutation did
not compile: exit 101 for unused market_id. That attempt is not counted as a
discrimination proof; it was restored and replaced by the compiling wrong-key
control above. Scoped working-tree source/test/script/manifest diff exited 0
after all restores. This sheet is manually recorded evidence, not test output
relabelled as a gate at an unmeasured SHA.

## Integration merge-down

Fetched and merged integration `55d8addaf74e5ff2c2da80ab19a03109b6fb9476`.
Merge SHA `50366986fcd870f12d0116cc71b7834559432010` has that integration as
second parent and reviewed evidence commit
`72d8ffe3351621e67342714ed1dab5e0c896be19` as first parent; merge was clean.
Scoped `git diff --exit-code 3e315f89dd HEAD --` both venues' src/tests,
KuCoin scripts and both venue manifest directories exited 0: byte-unchanged.
Shared exchange/num/types/store/transport/crypto and the TS venue methods also
have no input changes. Focused tests and eight controls remain bound to
3e315f89dd; they are not relabelled as tests run at the merge SHA.

Incoming Cargo.lock only adds dependencies for Coinbase and Weex. Both exact
scoped all-target cargo checks were nevertheless run fresh at 50366986fc:
OKX exit 0, KuCoin exit 0, zero errors. Fresh extractor check at 50366986fc
exited 0, inspected 7341 symbols / 512 source files and zero drift. Tomllib
re-measured against the merged integration: the same 4 pending->ported and
4 pending->divergent, with the unchanged triples in the initial table.
The following docs-only delivery commit preserves tested code/tests/scripts/
manifests byte-for-byte. No integration merge or PR creation was performed.
