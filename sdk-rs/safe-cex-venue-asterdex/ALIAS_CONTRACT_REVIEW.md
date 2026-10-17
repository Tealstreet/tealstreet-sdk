# Native alias contract review

This batch reviews the seven retained native/default AsterDEX aliases. Each
contract records the exact compiler generics, source target and expanded target.
Three bounded generic policies independently name the expected Rust type for a
single `serde_json::Value` instantiation; `ResultValue` retains its actual default
second parameter. No generic claim extends to arbitrary runtime values.

| Alias | Reviewed role |
| --- | --- |
| `BookCallback` | A stored subscriber receives a constructed order-book snapshot; delivery clones the book per listener. |
| `CandleCallback` | A stored subscriber receives a constructed public kline update. |
| `BookCleanup` | The returned zero-argument closure captures the stream/listener identity for unsubscribe cleanup. |
| `ResultValue<V, T = V>` | The actual standard `Result<T, V>`, including genuine Deserialize for the fixed JSON-value choice. |
| `Task<V>` | A pinned runtime future returns `ResultValue<V>` through start/adoption/await. |
| `Function<V>` | A retained runtime callback receives already-bound argument values and returns `ResultValue<V>`. |
| `AsterDEXHttpFactory` | A native dependency factory receives typed options and returns a client-host handle or exchange error. |

The reviewed declarations and consumers are in `public_subscriptions.rs`,
`runtime_host.rs`, `native_exchange.rs` and the concrete
`binding/host.rs`, `binding/mod.rs`, `binding/globals.rs` adapters. Their governing
boundary remains [owned snapshots versus live aliases](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).
This review does not execute subscribers, cleanup, tasks or HTTP
factories, and grants no raw-input entrypoint to those capabilities.

The fixed `ResultValue<serde_json::Value>` genuinely implements Deserialize and
therefore has a decoder contract. Independently authored inputs distinguish
`Ok`/`Err`, object/boolean/null/array payloads, unknown tags, multiple tags and
nonobject outer shapes. Native observations inspect the actual Result variant
and payload through owned Value, qualified borrowed Deserialize, JSON text and a
qualified text deserializer. They never serialize the decoded Result as proof.
This is a bounded native tooling-type contract, not a new venue input API.

The other six aliases have individual non-decoder reasons. Their canonical
native probes check actual compiler root identity, actual Deserialize absence
and all seven raw conversion traits, including the maintained narrow-static
conversion checks. The framework labels these executed capability receipts
`classificationOnly: true`, `category: non-decoder-alias`, `deserialize: false`.
They are reported as six native non-decoder alias receipts, separately from
the one decoder receipt; this is not a claim of seven decoder executions.

`scripts/accepted-alias-contracts.mjs` uses the ordinary full compiler inventory
before selecting these seven aliases. It does not use `onlyOwners`, alter the
rustdoc or suppress source/receiver refusals. Every selected external root is
authenticated against the same package's actual Cargo-check artifacts,
workspace metadata and sysroot. Macro origins come from that exact doc/artifact
set through `verifiedMacroOrigins`. The canonical `runProbes` path validates
the registered generic specs and emits invariant native type-equality witnesses
alongside the ordinary Serde/conversion probes. Compiler preparation uses one
job and the same pinned-toolchain configuration throughout.

The named `--root-control` follows the maintained native function-pointer
control: after normal contract validation, a process-local loader hook changes
only each emitted expected root string. All seven compiled native identity
assertions must fail at their named aliases. A normal rerun must pass again.
No production Rust, source document, receiver row or accepted permission is
changed by this control. Compiler errors do not count as native failures.

The historical eight-alias baseline included `ApprovalFuture`. Its unused browser
workflow module and accepted rows have since been removed. The maintained runner
now selects seven aliases: one decoder and six capability classifications. The
removed alias carries no native production binding claim.

This scope excludes other owners, local owners, feature configurations and
WASM execution. It does not claim the four-venue accepted-contract task is done.
