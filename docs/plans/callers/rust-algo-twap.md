# Rust TWAP core

`sdk-rs/algos/twap` (`safe-cex-algo-twap`) ports the TS worker's TWAP
(`TwapHandler`, `TwapInstance`, `TwapManager`, `TwapPostprocessor`,
`TwapOrderEnrichmentPostprocessor`) as a clock-driven state machine for the
native exchange server. Nothing calls it yet.

## Obligations

- Native server: drive one `TwapBook` per account through the shared algo
  runtime (`sdk-rs/algos/runtime`). Send `twap:*` ops to `handle`, wake at
  `wake_at`, report slice placements with `placement_settled`, and send
  `StateUpdate` as `twap:stateUpdate` and `Log` as an error log. The adapter
  follows in this crate.
- Chaser runtime: `StartChaser` needs start with a given id, plus progress by
  id and a remove that also cancels the open order.
- Web client: unchanged. It keeps persisting `twap:stateUpdate` and replays it
  as `twap:state` on connect.
