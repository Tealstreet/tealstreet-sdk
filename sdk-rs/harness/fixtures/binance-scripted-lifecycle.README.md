# Binance scripted lifecycle capture

`binance-scripted-lifecycle-private.jsonl` is a frozen, sanitized capture from
`live-smoke.integration.mjs`'s real WASM lifecycle running against its scripted
`fakeBinance` transport. No network request or live account was used. Capture
started at `2026-10-20T11:56:22.317Z`; its metadata fixes that replay epoch.

The file is copied verbatim from the successful capture. Client IDs, timestamps,
HTTP payloads and financial fields were not rewritten to make TS and Rust agree.
The recorder replaced signing credentials and the dummy listen key with replay
values. This is Rust-driven scripted coverage, not an independent TS recording
or evidence of live private parity.

| Artifact at capture | SHA-256 |
| --- | --- |
| Frozen JSONL, 15,342 bytes | `710a58aa511ed26bfe55bad1fef6b67a2544e5a119178560763261675d8862bf` |
| `live-smoke.integration.mjs` capture source | `2518faa18862055e5039a31b654eb8a1e7aec6ad4934f8db01048c0b5d351727` |
| Node WASM used to capture, 12,172,135 bytes | `51e335bd0acd407f75b51b26bf780d083ea49bd720b9533c2fc2cc9906c2d3c3` |

The 56 records contain six lifecycle commands: place, amend, cancel,
cancel-symbol, change leverage and restore leverage. They also retain the
recorder's separate authoritative verification session and its successful
cleanup result. The scripted upstream confirms no order remains afterward.

Two checks serve different purposes:

- `rust:test-live-smoke` generates a fresh capture and replays it through actual
  TS and Rust implementations. Both must complete all six commands, satisfy
  protocol and checkpoint coverage, deliver a book through production egress,
  and have no unmatched requests, missing writes or invalid signatures. Its
  fault cases still check cleanup, ambiguous placement, stale settings and a
  late fill. This generated-capture test makes no financial parity claim.
- CI also runs the unchanged strict financial gate on this fixed file as a
  mandatory ninth Binance case. The initial strict report is **FAIL**, with 22
  client-order-ID and time-in-force differences. The single
  `known-gate-failures.json` manifest may declare only the exact fixture hash and
  observed diagnostic paths. An accepted known failure remains a parity failure.

Run the financial comparison directly:

```sh
corepack yarn -s rust:gate --venue binance --tape sdk-rs/harness/fixtures/binance-scripted-lifecycle-private.jsonl --books BTCUSDT --runs 1
```

To produce a new diagnostic capture without overwriting this fixture:

```sh
RUST_LIVE_SMOKE_FAKE_TAPE_OUT=/tmp/binance-scripted-lifecycle-new.jsonl corepack yarn -s rust:test-live-smoke
```

Fresh captures have new wall timestamps and client IDs. CI never regenerates the
frozen financial fixture. Replacing it requires a new strict report and an
explicit update of its hash and observed diagnostics; generated financial
failures are never caught and silently accepted by the lifecycle test.
