# Crypto.com slice: 5 pending rows moved

**Moved 0 rows pending->ported, 5 pending->divergent**, measured with Python 3.12
tomllib over the six Crypto.com manifests, comparing integration reference
`2bc83d78d4f821c4b116d082d02c7b5a2595ffc5` with code SHA
`b6a3f1879aff2ca2f724d2887c503576c7d464de`.
Counts: **8 / 13 / 116 -> 8 / 18 / 111** (ported/divergent/pending), 137 rows.
The exact changed rows and manifest paths are in
[census-b6a3f1879a.json](census-b6a3f1879a.json). This is the Crypto.com manifest
census, not a claim about the global integration census after merge.

Code SHA's actual parent: `5cbe0fb0e4b0b915da7595de638da697f39d2d77`.
Inherited starting HEAD: `52e3cf8b8c253eaedc59958ac60856d05eef02d4`, with draft
`31ad96e4b8e487c2d600875534c5dd0e6a99dba6` as its parent.
Actual inherited integration base: `115f2f1c82077057764e859d6980d6c955a32315`.
The branch includes that inherited draft auth history; it was not rebased onto
the census reference. Static sub-agent reviews below cover the scoped witnesses
and implementation, not a claim that every ancestor was reviewed or integrated.
This report is recorded in a later evidence-only commit; exits below belong to
the named measured SHAs, not to that later commit.

## Rows and behavior

`5cbe0fb0e4` classified the three demonstrated auth functions using the existing
canonical CHOSEN signing execution/data/string boundaries. Their inline request
and WS result structs are explained mapping splits. The six witness comparison
compile fixes were already present at the inherited starting HEAD; fresh auth
and serializer mutation proof is bound separately to that SHA.

`b6a3f1879a` implements `isPublicEndpoint` and `isNonRetryablePrivateWrite` in
`api.rs`. Both retain source substring matching, including longer paths and
mixed public/private endpoint query text. The private-write helper's omitted URL
defaults to an empty string. Both rows are divergent with canonical CHOSEN UTF-8
notes. `createAPI`, actual retry decisions, signing interception, HTTP and socket
dispatch remain pending.

The endpoint oracle executes both complete source modules and exposes their
actual private helpers without modifying their bodies. Uncalled transport import
stubs do not stand in for those helper bodies. Rust directly compares native
bool results, with no decode/reencode comparison. Independent inventories admit
29 inputs and fixed expectations require 58 executed native comparisons.
The null capture marker denotes an omitted argument, not a tested JS null value.
The observed floor includes URLs for all nine public endpoints and all nine
private-write endpoints. Exact PUBLIC_ENDPOINTS membership still comes from the
existing constants witness: get-ticker is also a substring of get-tickers.

## Measured gates at b6a3f1879a

Exact commands, exits, elapsed times and raw log paths are in
[gates-b6a3f1879a.json](gates-b6a3f1879a.json). Logs carry the full measured SHA.
All Cargo commands ran sequentially; direct checks/tests/Clippy used jobs 2.
Oracle runs used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Exit | Observation |
| --- | --- | --- |
| Endpoint controls | 0 | Two compiling named failures, each exit 101; each restored pass exit 0 |
| Scoped `cargo check --all-targets --jobs 2` | 0 | Crypto.com and required dependencies only |
| Scoped full tests | 0 | 1 endpoint + 1 auth + 1 serializer + 3 types tests; zero unit/doc tests |
| Scoped format check | 0 | No format changes |
| Strict host Clippy, all targets | 0 | Host invocation |
| Strict wasm32-unknown-unknown Clippy, all targets | 0 | Distinct wasm invocation; compilation/lint, not wasm runtime tests |
| Canonical scoped parity | 0 | 137 rows, 37 public Rust items, no exemptions; 8 / 18 / 111 |
| Extractor writer | 0 | Output: updated 0 manifests, 0 added/orphaned |
| Extractor check | 0 | Output: would update 0 manifests, 0 added/orphaned; 7341 symbols / 512 files |

The two new controls in [endpoint-controls-b6a3f1879a.log](endpoint-controls-b6a3f1879a.log)
deliberately replace substring matching with equality:

1. `public_substring` compiled and failed `endpoint_native_public:3` (absolute URL)
   with exit 101, then restored and passed with exit 0.
2. `private_write_substring` compiled and failed `endpoint_native_write:24`
   (suffixed private-write URL) with exit 101, then restored and passed with exit 0.

Enumeration stopped after iteration **2**, `private_write_substring`, of the
present two-control list. No survivors in those controls. This is discrimination
for these defects and the named 29-input floor, not exhaustive URL coverage.
The tracked tree was clean before controls and after every restoration/gate.

## Earlier SHA-bound measurements

- At `52e3cf8b8c`, 19 auth controls and 16 serializer controls each compiled,
  failed their named runtime assertion with exit 101 and restored to exit 0.
  Scoped check/tests/format and separate host/wasm Clippy also passed.
  Canonical parity exited 1 with five genuine unmapped-item findings; extractor
  check exited 0. See [auth-52e3cf8b8c.md](auth-52e3cf8b8c.md). These control results
  are not relabeled to cover b6a3f1879a. Auth Rust source bytes are unchanged.
- At `5cbe0fb0e4`, the canonical parity gate exited 0 with 137 rows / 35 public
  items and census 8 / 16 / 113; extractor check exited 0 with no changes or
  additions/orphans. See [canonical-parity-5cbe0fb0e4.log](canonical-parity-5cbe0fb0e4.log)
  and [extractor-check-5cbe0fb0e4.log](extractor-check-5cbe0fb0e4.log).

Native sub-agent `/root/review_auth_witness` reviewed committed b6a3f1879a and its
parent mappings, finding no blocking issue. It verified matching semantics,
defaults, independent inputs/counts, the correct control indices, the documented
omission sentinel, and canonical notes/split targets. This was a static review:
the reviewer ran no Cargo, controls, extractor or canonical gates.

NOT RUN at b6a3f1879a: auth/serializer mutation matrices, configuration/wire/
cardinality/order control lists, process-count scripts, wasm runtime tests, full
workspace compilation, monorepo suites, CI reruns, live venue requests or actual
transport/retry/socket flows. Earlier process-count evidence remains at
52e3cf8b8c. No PR was opened and nothing was merged into integration.
