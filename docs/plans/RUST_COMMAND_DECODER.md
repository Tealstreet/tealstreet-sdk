# Rust Command Decoder

Owner: claude-fe7lgm. Scope approved by claude-ddi2il (size-only, wire format
unchanged). Independent review before source: codex-uufub4 or codex-4yjlw0.
Stacks after the engine-size PRs (`feat/rust-engine-size-items`,
`feat/rust-socket-interest-size`).

## Problem

`decode_command` deserializes `CommandIn`, a `#[serde(tag = "op")]` enum.
Internally tagged enums buffer the whole object into serde's private `Content`
tree, then replay it through `ContentDeserializer` for the chosen variant. That
instantiates a second deserializer path for every payload type: `PlaceOrderOpts`,
`Vec<_>`, `Option<_>`, `Interval` and the option and cancel types.

On Bybit, dazzy3's retained-size list puts `CommandIn` at about 158 KB raw
(379 functions, about 115 KB of it under `Content`). On WOO,
`CommandIn::deserialize` alone is 104,664 B raw. Every venue family pays it,
whether or not it serves the command.

## Design

Read `op` with one borrowed scan of the top-level object. Then deserialize only
the chosen variant's payload directly from the same text, with no `Content`.

1. **Tag.** A `Header<'a> { op: Op }` struct where `Op` is a fieldless enum
   with the same variant names, `rename_all = "camelCase"`, the `getAccount`
   alias and the same feature gates. Every other key is ignored. This
   reproduces `missing field \`op\``, `duplicate field \`op\`` and
   `unknown variant ..., expected one of ...` (same list, same order) from
   serde's own field-identifier code.
2. **Payload.** One plain struct per variant, with the same fields, defaults,
   renames and gates, parsed with `serde_json::from_str` from the same text.
   Unknown keys, including `op` itself, stay ignored, as today.
3. **Errors.** `Content` replays raise payload errors through
   `de::Error::custom`, so they carry no position (`line() == 0`). Direct
   payload parsing would add one. The decoder rebuilds such an error as the
   same message without the position, and the category stays `Data`.
   Syntax and EOF errors come from the reader in both paths, at the same
   offsets. The harness below decides which errors keep positions; source
   must match it, not this paragraph.

`decode_subscription` and `decode_host_input` keep their contracts.
`decode_host_input` still calls `decode_command`. `Command`, `CommandIn` →
`Command` mapping and every host are unchanged.

## Wire-compatibility harness (before source)

`engine/src/command_wire_compat_tests.rs` keeps a byte-for-byte copy of today's
derive-based `CommandIn` and its `From` mapping as a test-only oracle, `legacy`.
It compares `decode_command` against the oracle over a generated corpus. Each
input must give the same `Command`, or the same error `to_string()`,
`classify()`, `line()` and `column()`.

The corpus covers every `Command` variant in the build's feature set:

- the valid example (every variant must decode `Ok` under the oracle);
- the tag last and in the middle;
- unknown fields, including a nested object holding an `op`;
- tag errors: unknown, wrong case, number, null and empty `op`, and a duplicate `op`;
- for every payload field: absent, duplicated, and replaced by each of `null`,
  string, integer, float, negative, `u32` overflow, bool, array, object and
  exponent forms;
- whole-input cases: empty, whitespace, `{}`, `[]`, `null`, a scalar, a bare
  string, truncated, trailing characters, a trailing comma, a missing `op`, an
  `op` only in a nested object, `\u` escapes inside `op`, pretty printing and
  the host envelope's `timeoutMs`.

It runs in the default feature set and with every command feature
(`command-asterdex`, `-apexomni`, `-polymarket`, `-kucoinspot`, `-options`,
`-order-book`, `-schedule-cancel`, `polling-control`).

**State before source.** The harness is GREEN against today's decoder (it is
its own oracle), with a corpus of more than 1,000 inputs. A mutation check that
changes only the text of position-free errors makes it fail with 732
mismatches, so it detects error-shape drift, not just Ok/Err. This is the
compatibility gate the source must keep green. The size reduction itself is
measured, not asserted.

## Gates

- The harness green in both feature sets, plus the existing command,
  host-input, public-feed and native/WASM host tests.
- Optimized size measured on Bybit (dazzy3's stack) and WOO (#1930), before
  and after.
- CPU certified on the PR. One borrowed tag scan plus one direct payload parse
  replaces one parse into `Content` plus a replay, so CPU should not regress.
- Strict clippy, fmt, bans.
