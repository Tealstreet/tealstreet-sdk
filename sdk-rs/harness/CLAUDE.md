# Rust harness: replay evidence and WASM budgets

Sam removed the CPU gate matrix, shared certification, Bitget paired ratio gate,
planner and nightly monitor on 2026-11-17, for both master and pull requests.
The root `CLAUDE.md` records that decision; historical CPU thresholds and
certification instructions are not current CI requirements.

- Replay measurement and correctness jobs remain. Public replays retain CPU and
  memory evidence; private tapes remain correctness-only. Retained CPU metrics
  are observations, not the removed CPU gate statuses.
- Required coverage, actually executed combined consumers, strict replay
  validation and declared-known-failure checks remain required. A fast but
  incomplete replay cannot replace the audited correctness workload.
- The retained offline performance helpers still implement their historical
  comparisons, including the 10% retained-memory rule. Removing CPU gate jobs
  does not waive correctness, memory validation or WASM artifact budgets.
- Fingerprint movers need fresh optimized web/Node size evidence with at least
  10 KiB reserve, rounded up to KiB, while preserving historical size floors.

WOO's exact signature-error auth classification changes diagnostics only; socket
recovery, signing, retry and write behavior retain their lifecycle controls.
Exact source 85a85576995b, Rust SDK run 44220127210, artifact 14390881543
measures equal optimized web/Node/replay modules at 2,683,302 bytes. The
coordinator's Rule 5 floor is 2,694,144 bytes, retaining 10,842 bytes and every
historical floor.

OKX empty-envelope diagnostics retain code-0 cancellation continuation, nonzero
stop behavior and the original native outcomes. Its scoped ordered-write tests
pin complete per-order cancellation reporting and the explicit unknown-diagnostic
fallback; optimized family evidence remains separate from these native controls.

BingX Spot qualification adds independent Spot public/private cases to the `bingx` suite.
The nondefault `bingx-spot-replay` WASM feature exports a fixed-credential fixture constructor;
normal factories still refuse Spot. Shipped family size checks run before the fixture build.
The ordinary BingX family includes the existing book and symbol-cancel input
codecs. Offline production probes run through the shipped/fixture admission
entry point and require no dispatch of unadvertised commands; perpetual capability
flags and the Spot admission/smoke gates remain independent.

WASM venue caps retain both parents' measured floors. Fresh compositions must
retain at least 10 KiB above their optimized web/Node measurement, rounded up
to KiB; boundary tests retain inclusive cap and cap+1 refusal checks.

BingX history diagnostics inspect failed responses with closed streaming visitors.
Compact optional detail preserves engine error and socket allocation budgets;
fresh web/Node artifacts must retain the existing family cap and 10 KiB reserve.

TxFlow agent refresh #3314 at merge 9bb43250830d measures 2,147,684 bytes;
refusal fix e959e2a7dc2e measures optimized web/Node 2,147,758 bytes. Its
2,158,592-byte floor retains the required 10 KiB reserve at that source.
The review fix based on 2c37c0e6af2d measures optimized web/Node 2,148,376 bytes;
its 2,159,616-byte floor retains 11,240 bytes. The private tape supplies the
owner-bound disposable agent approval before writes. Replay identity equates only
absent and explicit default `signatureChainId:2511`; economic action fields and
nondefault chain values stay exact. Signing validation remains independent.
WEEX in the same composition measures 2,028,105 bytes and retains a 2,038,784-byte
floor by the same rule. Merge f19209120d28 measures one byte more in each
family (2,148,377 and 2,028,106 bytes), retaining those floors and 10 KiB reserves.
Pinned profiles and remaining family/global budgets are unchanged.

TxFlow construction-feature guards require dispatch signing, address/nonce setup
and its hosted-wallet codec, with independent missing-feature refusals. The HTTP
HMAC hosted-signing feature cannot substitute for that testnet wallet codec.

The #3314 serial-placement fix at 1c2a4f8232c4 measures optimized web/Node
2,149,614 bytes. Its 2,160,640-byte floor retains 11,026 bytes using the same
pinned profile and 10 KiB reserve rule; older measured floors remain retained.

Merge ebeb0b38c973 independently remeasures the same TxFlow and WEEX byte
counts (2,149,614 and 2,028,106), retaining both floors and the pinned profile.
The exact merged module hashes are recorded beside their budget entries.

Bybit product parity fixtures service the engine's set/clear timers and retain
original waiting HTTP permits until release or cancellation. Their monotonic
virtual clock also validates native signature timestamps; waits never refresh deadlines.

Shared artifact membership is frozen in `sdk-rs/artifact-census.json`. Additions
and retirements update that independent census explicitly; value-only edits do not.
Budget imports validate raw duplicate/name membership before evaluating the map.
The closed source scanner ignores nested template text and expressions, rejecting
unsupported regex/division syntax instead of guessing their lexical boundaries;
construction feature checks reject raw duplicate keys and missing policy rows.

Bybit terminal order-history retention is documented in `sdk-rs/venues/bybit/README.md`.
Its `history.rs` change is a counted beta-WASM input and requires a fresh release/pin;
this routing correction does not change family caps or construction membership.
