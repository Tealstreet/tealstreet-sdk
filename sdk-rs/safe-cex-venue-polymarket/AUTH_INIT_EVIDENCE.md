# Polymarket signing helpers and constructor bodies

Moved **8 pending -> ported, 0 -> divergent**. Tomllib census:
**99/18/24 -> 107/18/16** (ported/divergent/pending). Independent141-row,
seven-manifest floor, stopped at polymarket.ws-public.ts.toml. This is symbol
census, not proof that every Polymarket runtime path is implemented. Exact
changed rows/targets are in gates/census.json.

Original integration parent: `d1652df92999c41782adbd5be274485279db548b`. Implementation and all
initial gate/control measurements: **`13afb38e6e9a25f183f4566c083f863b72797ea3`**. Branch:
`feat/polymarket-runtime-init-d1652d`. No unpublished foreign branch was used
as an ancestor; later integration merges are recorded separately.

Five actual signing helper bodies and API/private/public constructor bodies
are callable Rust functions. Constructor bodies operate on their native
receiver/base bindings with ordinary writes and current slot reads. The API
factory, complete classes/allocation/declaration initializers and callback
storage declarations stay pending. No placeholder Rust class/field target is
invented to move a row. Ethers/official SDK/HTTP factory/retry/BaseWebSocket
are imported dependencies, not reimplemented in this slice. Wallet creation,
EIP712 signing and official V2 order construction remain bound library calls.

## Measured gates

All command/exits are in gates/gates.json, attached to `13afb38e6e9a25f183f4566c083f863b72797ea3`.

| Gate | Exit |
| --- | --- |
| source-oracle | 0 |
| check | 0 |
| tests | 0 |
| fmt | 0 |
| targets | 0 |
| writer | 0 |
| extractor | 0 |

Scoped check: all targets of safe-cex-venue-polymarket, locked/jobs2, zero
errors. Full venue tests: **119 passed** (prior independent111 + eight new),
stopped after ws_timer_clear and zero doc-tests. Source oracle: **96 fixtures**
(eight x twelve), stopped at publicConstructor/11. Target indexer: real
venue-scoped invention/status gates, empty exemptions,141 rows and347 public
items (320 prior + eight free functions + WalletSigner/type and two methods +
AuthHost/type, associated Constructor and14 methods). Writer/extractor output
explicitly says zero updates/additions/orphans; exits alone are insufficient.

## Native discrimination

Baseline eight tests exit0. Eight production faults compiled, failed the named
native assertion with exit101, restored the exact original source in finally,
and passed with exit0. Stopped after publicConstructor. No survivors, wrong
failures or abandoned runtime controls. Previously ported rows' control
matrices were not rerun.

| New family | Exact test | Native assertion | Baseline / broken / restored |
| --- | --- | --- | --- |
| createWallet | `wallet_native` | native text wallet private key | 0 / 101 / 0 |
| createSignedOrder | `signed_order_native` | native constructor captured before parameter getter | 0 / 101 / 0 |
| generateHmacSignature | `hmac_native` | native independent UTF8 message bytes | 0 / 101 / 0 |
| buildL2Headers | `headers_native` | native text L2 wallet address | 0 / 101 / 0 |
| signApiKeyDerivationMessage | `auth_signature_native` | native text auth domain name | 0 / 101 / 0 |
| apiConstructor | `api_constructor_native` | native bits client timeout | 0 / 101 / 0 |
| privateConstructor | `private_constructor_native` | native private credential callback alias | 0 / 101 / 0 |
| publicConstructor | `public_constructor_native` | native original public/private parent alias | 0 / 101 / 0 |

Native decoded values/own-key schemas, bits and original input pointers are
asserted before wire comparisons for every fixture in each family. Independent
key/message byte vectors and fixed digest literals are checked directly on
native values; no decode/reencode equality is native proof. Digest literals
were generated with Node crypto.createHmac over fixed bytes/messages, separate
from the Rust helper/bound crypto implementation and actual-ethers oracle.

The source oracle extracts actual helper declarations, auth constants and
constructor bodies. Its HMAC/UTF8/base64/byte calls use installed real ethers;
Rust fixture bindings use safe-cex-crypto and base64. SDK Side comes from the
installed official module. Wallet, SDK builder/guard, HTTP factories/retry and
BaseWebSocket are explicit spies. Real ECDSA/private-key creation/V2 builder
rounding/negative-risk-contract signing or network execution is not claimed.

Native proof covers URL-safe secret normalization/padding, omission versus
false/null/zero/empty body, timestamp defaults and signedzero/nonfinite bits,
all five L2 headers, all three ClobAuth domain fields/four typed fields/four
value fields/three return fields, original shared auth-types identity with
repeated-call mutation, original wallet receiver and bridge argument pointers,
late wallet methods, pre-argument constructor capture, original signed return
and guard rejection, API client configs/header schemas/retry callbacks,
constructor assignment prefixes and current gamma receiver slot.

## Constructor emit and scope

The oracle uses the configured modern TypeScript target/default define
semantics. Its API wrapper synthesizes the parameter-property own slot before
body execution, modeled by the supplied native receiver's initial undefined
getCredentials slot. That write is checked independently in native prefix and
alias assertions; it does not invoke an inherited setter, so it is absent only
from the observable source setter-event projection. Other class declaration
initializers are intentionally not included by constructor-body extraction and
belong to pending class rows. Setter/current-slot witnesses here prove the
supplied receiver/body contract, **not inherited-setter behavior of the complete
configured classes**. No full class completion is implied by a constructor row.

## Review and development findings

Static sub-agent review ran no compiler/oracle/gate/control and stopped all
readers before mutation execution. It found and prompted corrections for:
shared CLOB_AUTH_TYPES identity, ordered receiver writes/current client reads,
constructor capture before argument getters, native bridge pointer checks,
an indistinguishable alternate-constructor spy, and explicit nonconfigured
class emit mode. The alternate spy now emits a distinct event and the source
asserts it was not used. No additional production mismatch was reported at
that review floor. Two proposed controls were corrected statically before
execution: wrong-Text wallet input matches the named assertion; replaced header
address is consumed to preserve successful compilation. They are not failed
runtime control attempts or survivor findings.

Development native exits101 for two borrowed temporary test strings; native2
exits0 with eight tests. Those uncommitted runs are not gates for a commit.
Formatting yielded once, and a focused development test was started before I
had confirmed formatting completed; formatting later returned0 and that test
returned101. That serialization lapse is disclosed, not excused by its exits.
All seven committed gates and all controls executed serially, with each measured
Cargo completion confirmed before the next command. Source/writer development
outputs were also read; writer reports zero updates. Raw bytes and SHA256/length
receipts are retained in artifacts.json; compressed logs are unedited.

## Not run

No workspace build/test/sweep, host/wasm clippy, wasm checks, monorepo suites,
CI reruns, live venue requests or imported-library signing implementation tests.
Shared SDK types/traits were not edited. Only venue-local AuthHost and
WalletSigner were added; Cargo adds two existing workspace dev dependencies.
Counts/fixtures/controls are measured floors with explicit stops, not a claim
of arbitrary values, getters/proxies, full heap graphs or scheduler completeness.

## Delivery preservation

Push-time integration merges, exact source/test and broader input differences,
and any fresh gates are attached separately below/alongside this sheet. They
never relabel the initial gate/control SHA. Evidence-only commits do not change
the inputs used by the recorded gates.

Merged integration `372ef194e884c18cb92533b00a097952a00304b5` down at
`09656b2780c1d7168fbbcbbea06f343046bafaaa`. Source/tests diff against the
initial gate/control SHA exits0. The34-path input audit preserves strict
differences for foreign global manifests/exemptions/specification and a foreign
package script change. All our Rust/tests/Cargo/manifests, shared dependency
code, lockfile, TypeScript sources and gate/indexer/extractor code are byte
unchanged. Fresh seven scoped gates at this merge SHA all exit0: source96,
all-target check, native tests8, fmt, targets141/347, writer and extractor.
Output text confirms zero metadata changes. Full119 venue tests and eight
mutation controls were not repeated; their exits remain attached to13afb38e6e.
See push-gates/gates.json, census.json and preservation.json.

Final integration merge `74dcf0642ea1ffad76a375cb11b72f7c4465c9a3` incorporates
`2691078b11e3bff6d4541ede634a3ed1e54627ab`. The34-path comparison to09656b2780
found only foreign global manifests differing; source/tests, dependencies,
our manifest and all other inputs stayed byte unchanged. Exact combined
source/test preservation command exits0. The affected global extractor was
remeasured at74dcf0642e: exit0, output zero updates/additions/orphans. Scoped
compile/test8/fmt/target/writer gates were not repeated after this metadata-only
merge and remain attached to09656b2780; full119 and controls remain at13afb38e6e.
See final-metadata/gates.json and preservation.json.
