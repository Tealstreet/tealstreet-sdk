# Auth witness recovery measured at 52e3cf8b8c

Measured SHA: `52e3cf8b8c253eaedc59958ac60856d05eef02d4`.
Direct parent: `31ad96e4b8e487c2d600875534c5dd0e6a99dba6` (inherited draft).
Actual inherited integration base: `115f2f1c82077057764e859d6980d6c955a32315`.
The branch includes the inherited draft auth commits after that integration base;
it was not rebased onto the priority brief's `2bc83d78d4`.
This report and subsequent manifest classifications were not present at the
measured SHA. These results are not relabeled as gates at later commits.

At the measured SHA, tomllib over all six Crypto.com manifests found
**8 ported / 13 divergent / 116 pending**, 137 rows. The same scoped census at
`2bc83d78d4` is identical: **0 pending->ported, 0 pending->divergent at this SHA**.
The later annotation commit moves the three implemented auth functions.

The fresh agent found no tracked diff or leftover mutation. The comparison fix
was already committed at this SHA. The six E0277 errors measured at `31ad96e4b8`
are historical compilation failures, not runtime mutation proof.

## Measured exits

Exact commands and exits are in [gates-52e3cf8b8c.json](gates-52e3cf8b8c.json);
each linked raw log starts with the measured SHA and command and ends with exit.
Oracle runs used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
All Cargo commands were sequential. Check/tests/Clippy used jobs 2; existing
control/process/parity scripts use jobs 1.

| Gate | Exit | Observation |
| --- | --- | --- |
| Scoped `cargo check --all-targets --jobs 2` | 0 | Crypto.com crate and required dependencies only |
| Scoped full tests | 0 | 1 auth, 1 serializer, 3 types tests; zero unit/doc tests |
| Scoped format check | 0 | No formatting change |
| Strict host Clippy, all targets | 0 | Separate host invocation |
| Strict wasm32-unknown-unknown Clippy, all targets | 0 | Separate wasm invocation; 10m58s compilation |
| Auth controls | 0 | 19 runtime failures at named assertions, each exit 101, each restored pass exit 0 |
| Serializer controls | 0 | 16 runtime failures at named assertions, each exit 101, each restored pass exit 0 |
| Node process count: auth | 0 | 1 Node invocation, 1 test |
| Node process count: serializer | 0 | 1 Node invocation, 1 test |
| Node process count: types | 0 | 1 Node invocation, 3 tests |
| Extractor check | 0 | Output: 7341 symbols / 512 files; would update 0 manifests, 0 added/orphaned |
| Canonical scoped parity | 1 | Five real invention findings listed below |

Canonical parity found existing unrestricted public items without manifest
mappings in `src/signing.rs`: `generate_signature`, `generate_authenticated_request`,
`generate_ws_auth_message`, `AuthenticatedRequest`, and `WsAuthMessage`.
The three corresponding source functions were still pending/empty. No exemptions
were added and no gate was weakened. Extractor coverage being green did not make
this canonical gate green. Subsequent classifications address these findings.

## Runtime discrimination floor

[Auth controls](auth-controls-52e3cf8b8c.log) stopped after iteration **19**,
`ws_return_key`, of the current list. Every mutation compiled and failed
`live_auth` at its required named assertion, then passed after exact restoration.
This includes `clock_getter_bypass` -> `auth_native_id`, the compound
`signature_without_admission` -> `auth_native_signature:0`, params projection ->
`auth_native_params_identity`, and the error-name mutation ->
`auth_native_error_name`. No survivors in these 19 controls.

[Serializer controls](signing-controls-52e3cf8b8c.log) stopped after iteration
**16**, `walk_subset`, of the current list. All compiled, failed their named
runtime assertions and restored to passes. No survivors in these 16 controls.
These are enumeration floors, not complete mutation coverage.

The auth witness executes 42 admitted cases from seven input groups, with fixed
branch expectations `[14, 14, 14]` and four errors independent of the oracle's
length. All native fields, identities, signatures, errors and execution counts
precede output-field-list assertions. Serializer coverage remains 280 cases.
No decode/reencode roundtrip is used as native-value evidence.

Native sub-agent `/root/review_auth_witness` statically reviewed the committed
SHA and found no blocking witness correctness issue. It independently identified
the five unmapped public items. It ran no Cargo or mutations; runtime results
above were measured by the primary agent.

NOT RUN: full workspace compilation, monorepo suites, CI reruns, separate host or
wasm Clippy for other crates, configuration/wire/cardinality/order control lists,
live exchange requests, HTTP serialization, retries or socket dispatch. No such
claim follows from this report. New endpoint classifiers were not present here.

The inherited logs remain in the worktree. Actual root glob `.cryptocom-*.log`
matched **52**, rather than the brief's 54; none was discarded. Their local names,
sizes and hashes are recorded in
[predecessor-log-inventory.json](predecessor-log-inventory.json). Their earlier
results have not been relabeled to cover this SHA.
