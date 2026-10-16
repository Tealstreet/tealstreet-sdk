# Binance broader gates and finding repair

PR: https://github.com/Tealstreet/tealstreet-next/pull/7851.
Actual integration parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.

## Frozen baseline

All commands below ran serially against pushed implementation/evidence revision
`69418891fe2c3ffc28a6d558dd98641a3b94325e`. No tracked source changed during
the run. Raw records are in [gate-evidence/integration-gates](gate-evidence/integration-gates).

| Gate, from `sdk-rs` | Exit | Result |
| --- | ---: | --- |
| `cargo fmt --all --check` | 0 | Workspace formatting |
| `cargo clippy --workspace --all-targets --locked --jobs 2` | 101 | Four Binance library sites reached; later targets unclaimed |
| `cargo build --workspace --locked --jobs 2` | 0 | Native workspace build |
| `node --test ../.github/scripts/rust-wasm.test.mjs` | 0 | Wasm policy coverage audit |
| `node ../.github/scripts/rust-wasm.mjs --build` | 0 | Build of packages/features declared in the wasm policy |
| `node ../.github/scripts/rust-wasm.mjs --clippy` | 1 | Wrapper reports underlying Cargo 101; four Binance library sites reached |
| `cargo test --workspace --doc --locked --jobs 2` | 0 | 25 passed, zero failed/ignored across 39 independent manifest-derived library targets |

The doctest enumeration stopped at `safe_cex_webworker`. Host and wasm lint
results are separate measurements. Their initial four-site floors coincide:
`PublicEventArray` lacks `is_empty`, two private-class calls wrap
`class_jobs::reaction` in redundant closures, and one exchange-class call wraps
`get` unnecessarily. Neither failure enumerates every workspace lint site.
Subsequent keep-going runs must retain any newly exposed sites as findings.

The previously excluded broad commands are now measured. Their pass results
apply to this baseline; the lint failures still need correction. No CI rerun or
live venue request was made.

## Finding inventory and investigation

The frozen baseline has 252 ported / 34 divergent / zero pending rows. The twelve
pending integration-parent rows already have concrete implementations and
explicit divergent notes in the earlier slice; see [GATE-CLASS-OWNERS.md](GATE-CLASS-OWNERS.md).

At that baseline, literal `FINDING` text occurs in twenty rows: sixteen legacy socket rows
(six private open/ping, five public open, five statistics wrappers), one
historical mapper, and three exchange owners added by this branch. This is a
text census, not proof of closure. The inventory examines twenty files and stops
at `binancespot.ws-public.ts.toml`; source notes are referenced by hashes rather
than copied into a second declaration.

Read-only local sub-agent review found that all eleven notification findings
still expose a real core defect: `BaseWebSocket::send` synchronously propagates
`reconnect` failure, preventing local diagnostics, resets or subscribe/open
continuation. The actual TypeScript call discards the async reconnect promise.
Existing `send_store` tests exercise only TypeScript; their native parity gap
cannot be closed by those tests. Store error conversion also loses the original
opaque callback cause.

The five statistics findings require actual shared mutable prototype referents,
including later cross-socket reconciliation errors. Historical mapping must
preserve inherited native values, including prototype objects, before typed
narrowing. Saved references must retain their originating realm. These remain
open until the native implementations and discriminating assertions pass.
Classification follows [the canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy);
typed adaptation cites [the output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary).

Source-only exploratory runs at the baseline exited 0 for the existing 64-case
send witness and 1280-case statistics witness, stopping at cases 63 and 1279.
Those explorations are explicitly not native parity gates. The forthcoming
repair tests must assert native values before running source comparisons.

## Lint follow-up before shared-core repairs

The four initial library lint sites were corrected in `f2d3118044`. At pushed
SHA `65e4cca3b39aa72e570f2e8f43b2ccecd929f8ba`, native workspace clippy with
`--all-targets --locked --jobs 2 --keep-going` completed with exit **0**.

The independent policy-selected wasm clippy run was interrupted with SIGINT
before completion; its wrapper returned **255**. It is **not a pass** and does
not enumerate all lint sites. Its exact effective Cargo arguments and incomplete
status are preserved in the JSON record. Source inputs remained unchanged
during both runs.

The notification and prototype repairs change shared-core inputs, so both full
lint lanes must run against their repaired SHA. The baseline build/doctest
measurements above remain bound to their original revision. No result is being
retagged to later source.

## Notification follow-up

The later [notification gate sheet](NOTIFICATION-GATES.md) binds the repaired
send/Store behavior to its own measured implementation SHA. Eleven divergent
notification rows now move to ported: 263 ported / 23 divergent / zero pending.
The remaining literal FINDING census is nine, including five statistics wrappers,
one historical mapper and three owners. The broad measurements above retain
their original SHAs; they do not cover the later notification source.

## Prototype follow-up

The later [prototype gate sheet](PROTOTYPE-GATES.md) records the repaired native
statistics/history/Store behavior at its actual SHAs and20 named negative/restored
control pairs. Five further Binance rows and three shared-core rows move to
ported. Current venue census is268 ported /18 divergent /0 pending, with three
owner FINDING rows remaining. Earlier workspace/wasm/build/doctest records above
retain their original SHAs. Final broad commands for the repaired source remain
owed; none is retagged by this update.
