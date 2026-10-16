# Asterdex private listen-key lifecycle: eight rows

Branch `feat/rust-asterdex-listen-key-8`, actual starting parent
`9916944cd54d8861db6b81f96878fa6082ff4739` (integration; PR 7630 merged).
No unreviewed lane was inherited. Full private socket class, constructor,
connection and message rows remain pending; their assembly is not claimed.

## Movement and boundaries

Independent `tomllib` census `scripts/listen-key-census.py`, iteration 1,
stops at eight Asterdex manifests / 231 rows:

| Status | Parent | Slice |
| --- | ---: | ---: |
| ported | 118 | 121 |
| divergent | 55 | 60 |
| pending | 58 | 50 |

**3 pending -> ported, 5 pending -> divergent.** The eight named source methods
have distinct real targets in `listen_key.rs::ListenKeyLifecycle`. The three
clear methods cancel actual timeout registrations, preserve other state and
have deadline-crossing witnesses. Five distinct CHOSEN notes document typed
HTTP/error seams, caller/owner future cancellation, owned timer cleanup,
normalized integer delays and native exception reporting on their own methods.
The implementation retains persistent key, wall-clock stamp, binary64 failure
and retry counters, fired-handle semantics, refresh replacement, watchdog
rescheduling and connection-retry state. Reconnect/connect calls are parent
hooks, not fabricated complete socket implementations. No shared field or
existing shared trait was changed, and no query-value projection was introduced.

## Behavioral floor and review

The oracle extracts all eight unedited TypeScript method bodies and state/delay
fields. Controlled Axios, connection and timer seams capture actual POST/PUT
paths and keys; PUT explicitly requires its undefined body. Constructor and
transport integration remain outside this member-level floor.

Iteration 1 stops at **six scenarios / 68 snapshots / 36 RPC dispatches**.
Independent literal names and per-case counts precede comparisons. Direct
native key, failure/retry counters, successful wall timestamp, registration
flags, live timers, dispatched methods/paths/keys, reconnects, returned keys
and errors are asserted before emitted logs. No roundtrip/re-encoding gate.

Cases cross the successful 1800000ms refresh deadline, replacement at 50ms,
first failure at 30000ms, the capped 300000ms failure retry, missing key,
POST rejection and overlapping PUT completions; watchdog checks the exact 4200000ms threshold versus
strictly greater elapsed time and handles backward wall-clock
movement, clear/restart; connection retries preserve count across cancellation,
cross attempts two through fourteen including the 60000ms cap, and observe
connection rejection. Native owner cancellation has a separate test, with its
source timing difference explicitly disclosed rather than compared as parity.
This floor does not enumerate arbitrary schedulers or every malformed payload,
JavaScript thrown value, host-hook exception or numeric delay.

Read-only sub-agent review found no ordinary-path body mismatch. Its two
observation gaps (constant spy paths; missing first/success refresh deadlines)
were fixed and reviewed again. It ran no Cargo or controls. An initial fixture
check failed E0308 (101), then the task result mapping was fixed before the gates
below. An uncommitted smoke run was interrupted (130) while the compiler loaded
a dependency dylib; it is not a test pass or a completed gate claim.

## Measured gates

All four code gates measured at **`25c5353df45e8cc342bdded55212d42aaf933ce9`**.
Exact argv, raw outputs and environment are in `LISTEN_KEY_EVIDENCE.json`.

| Gate | Exit |
| --- | ---: |
| Scoped Asterdex cargo check, all targets, jobs 2 | 0 |
| Full Asterdex cargo tests, all targets, jobs 2, include ignored, test threads 2 | 0 |
| Extractor check: zero updates/additions/orphans | 0 |
| Independent tomllib census, notes/targets | 0 |

Compiled production controls are recorded in `LISTEN_KEY_CONTROLS.json` at that
same SHA. Each failure reached its named assertion, and exact original bytes
were restored before the positive run:

| Control / assertion | Broken | Restored |
| --- | ---: | ---: |
| Key corruption: `refresh-success-replacement native listen key` | 101 | 0 |
| Stale threshold > to >=: `watchdog-exact-stale-boundary native last` | 101 | 0 |
| Retry increment 5000 to 1000: `connect-retry-cap-and-clear request count` | 101 | 0 |

Iteration 1 stops at three controls, one per new family; **zero survivors**.

Not run: workspace compilation, other venue suites, host/wasm clippy, wasm
builds, real-network calls, full monorepo suites, CI reruns, or complete socket
constructor/connect/message behavior.

## Merge-down preservation

Merged integration `d5c0e7d58a7f03c8163680335e7b632ebb43ec86` at
`8fbfda59416bd7374ea08d5d7b62efe8a8ab598c`. The required source/tests diff
against gate SHA `25c5353df45e8cc342bdded55212d42aaf933ce9` exited **0**.
The extended comparison of crate scripts/manifests, live TypeScript inputs,
Asterdex parity manifests, workspace/toolchain configuration and the eight
reachable local Rust package sources/manifests also exited **0**. The reachable
Asterdex Cargo.lock graph compared equal (exit **0**); no lockfile package changed.
Exact scopes and the comparison script are embedded in the JSON evidence.

Cargo exits remain measurements at the original code SHA, preserved by these
input comparisons; Cargo was not rerun at the merge SHA. At the merge SHA,
extractor check exited **0** with zero updates/additions/orphans, and the tomllib
census exited **0**, reconfirming eight moved and 50 pending. Commands and raw
outputs are recorded under `post_merge_gates`.

Final fetch merged integration `536f757500a6eb348752e23387c098e3e9d9e3b0`
at `fe2ac6fc0d053001621ebc780db565a882529fa7`. Coinbase/exemption accounting
changed; all Asterdex source/tests, extended inputs and reachable lock graph
comparisons again exited **0** against the same code SHA. Final-merge extractor
and tomllib census each exited **0** at this merge SHA; the extractor output
again confirms zero updates/additions/orphans. These results replace the
`post_merge_gates` entries, with the earlier merge records retained separately.
No Cargo rerun is claimed for either merge.
