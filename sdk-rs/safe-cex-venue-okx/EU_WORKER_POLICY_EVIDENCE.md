# EU market and worker policies

This fresh slice moved **5 pending to ported, 1 pending to divergent**:
one ported OKX worker policy, one CHOSEN OKX EU predicate, and four ported
KuCoin private callback algorithms. Fresh branch feat/okx-kucoin-market-policy
started exactly at integration 17b6bcd0db760cfb175d1f5e21e581b5d29bd2a5.
No unreviewed feature ancestor was adopted. Tomllib independently parsed all
twelve manifests per venue:

| Venue | Integration ported/divergent/pending | Slice ported/divergent/pending |
| --- | --- | --- |
| OKX | 50 / 18 / 95 | 51 / 19 / 93 |
| KuCoin | 67 / 39 / 85 | 71 / 39 / 81 |

Implementation gate SHA: 5ec7cc1a98b44e7bc79a4ef05ddffd313544e723.
Native error-pointer test refinement/control SHA:
0603b88a34140af7f6a7cc454499c5b33a825394, parent 5ec7cc1a98.
OKX src/tests/manifests are byte-unchanged between these SHAs (scoped diff 0).
Exits remain attached to the actual measured SHA, not a future delivery commit.

## Measured gates

One cargo at a time, --jobs 2; venue tests set
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Gate | Measured SHA | Exit / scope |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2` | 5ec7cc1a98 | 0, zero errors |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 5ec7cc1a98 and 0603b88a34 separately | 0 each, zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test eu_worker_policy` | 5ec7cc1a98 | 0, one focused test |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --jobs 2 --test ws_helpers ping::private_socket::` | 5ec7cc1a98 and 0603b88a34 separately | 0 each, three focused tests; nine filtered out |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 5ec7cc1a98 | 0; inspected 7341 symbols / 512 sources, zero changes/additions/orphans |
| Rustfmt --edition 2024 --check on new source/test Rust files | 0603b88a34 | 0 |

Manifest edits used parseManifest/serializeManifest followed by parity:extract.
Writer exited 0, inspected zero changes/additions/orphans; no hand-spelled arrays.
Existing oracle discovery includes the additional targets without package edits.

## Native assertions and exact scope

EU filtering: 144 independently declared own-record profiles, six instrument
values x six contract values x four rules. Literal index expectations pin
booleans before source comparisons. Floor stops at array FUTURES / object
contract / false rule. The oracle retains actual Standard/EU inheritance and
both real getInstrumentTypes methods. CHOSEN: source Object.create with inherited
FUTURES/xperp/linear fields returns true, while its own JSON view {} returns
false natively. The port keeps decoded own JSON records; adding an executable
prototype/getter runtime inside wire market filtering is the declined choice.

Worker completion: 36 native enum/state identities precede serialization,
three initial modes x four exact UTF16 method names x three boolean/absence
profiles. Only changePositionMode changes mode; the unused result is omitted
by the adapter. Floor stops at initial Hedged / lone D800 method / true first
argument. Declared boolean arguments and absent first arguments are covered;
no arbitrary non-boolean changePositionMode argument claim is made.

KuCoin: see safe-cex-venue-kucoin/PRIVATE_LIFECYCLE_EVIDENCE.md. Ported rows
claim local callback algorithms. Concrete socket classes remain pending.
Injection does not repair or discharge the existing non-OPEN core reconnect
error-ownership seam. No closed-core interoperability claim is made.

## Discrimination controls

All controls ran at 0603b88a34. Each production mutation compiled, failed its
named native assertion with exit 101, finally restored the file byte-for-byte,
and the same exact focused test passed with exit 0. None survived.

| Newly moved row | Mutation | Named failed assertion | Broken / restored |
| --- | --- | --- | --- |
| EU isSupportedMarketInstrument | xperp requirement changed to normal | `OKX-MARKET native EU support 2/1/1` | 101 / 0 |
| afterMethodCalledInWorker | true maps to OneWay | `OKX-WORKER native mode None/0/Some(true)` | 101 / 0 |
| futures private subscribe | first fixed topic changed | `KUCOIN-PRIVATE native topic kucoin/0` | 101 / 0 |
| spot private subscribe | first fixed topic changed | `KUCOIN-PRIVATE native topic kucoinspot/0` | 101 / 0 |
| futures private onOpen | reset subscribed to true | `KUCOIN-OPEN native reset before subscribe` | 101 / 0 |
| spot private onOpen | snapshot batch 10 changed to 11 | `KUCOIN-OPEN native call order kucoinspot/false/false/true/none` | 101 / 0 |

Exact tests: eu_market_and_worker_policy_native_before_source,
ping::private_socket::private_subscriptions_native_before_source, and
ping::private_socket::private_open_native_before_source, each with --exact.

## Review and non-claims

Static sub-agent review has no remaining findings and ran no gates. It caught
an uncommitted filename collision replacing existing market_policy.rs /
market-policy-oracle.mjs tests. Both originals were restored byte-for-byte from
17b6bcd0db; scoped diff exited 0. New tests use eu_worker_policy.rs and
eu-worker-policy-oracle.mjs. Existing 401-case gates and cardinality checks
were preserved and were not rerun. The new corpus's first pre-commit validation
used the collided name and exited 0; it is not reported as the existing gate.
Native callback same-error captures now derive from Rc::ptr_eq, not literal true.

Not run: workspace build, full venue/monorepo suites, other venue test targets,
Rust parity suite, host Clippy, wasm build/Clippy or CI. No new mutation matrix
was run on previously ported rows. Guidance/agent-memory were not re-read.
Merge-down and scoped survival proof follow before delivery.

## Integration merge and fresh gates

Clean merge 728877aada2fd3c7376ef942fc2541dc5d309673 has second parent
f3189bc39dbbb104821ef45cdcc838c99a63c174 (fetched integration) and first
parent fb532bd8297895abaf09543d90a063c45108ce8c. Scoped diff against
0603b88a34 for both crates' src/tests, KuCoin scripts and both manifest
directories exited 0: byte-unchanged. Shared crypto/num/store/exchange inputs
changed, so both exact scoped all-target checks and both focused test commands
above ran fresh at 728877aada: all four exits 0, four tests passed, zero check
errors. Fresh extractor check at that same SHA exited 0, inspected 7341 / 512
and zero changes/additions/orphans. Controls remain measured at 0603b88a34,
not relabelled as controls at the merge. Tomllib against the actual merged
integration confirms the unchanged six-row delta and triples in the table.

Final fetch merged exact FETCH_HEAD integration
3e4e053b47fbfa44475712b68fdf53c23f5071cd in clean merge
b36154c325fef129b4175c7aba6f3580fa44e4c9; first parent
bd4025bde6c60590ad21be53a544be31032105fe. Scoped diff against 0603b88a34
again exited 0 for owned src/tests/scripts/manifests. Incoming core added
has_websocket, so both scoped all-target checks and both focused test commands
ran fresh at b36154c325: all four exits 0, four focused tests passed. Extractor
also exited 0 at that SHA, inspected 7341 / 512 and zero drift. Tomllib against
3e4e053b47 re-confirms OKX 51 / 19 / 93, KuCoin 71 / 39 / 81, delta 5 ported /
1 divergent. The following evidence-only delivery commit preserves the tested
files byte-for-byte. No PR was created or merged into integration.
