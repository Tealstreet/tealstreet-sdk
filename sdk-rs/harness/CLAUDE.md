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

HyroTrader correlated auth-refusal metadata changes a Bybit-family input.
Keep its native admission/uncertainty controls and fresh optimized family evidence;
server negative-join fixtures retain the protected-provider and zero-write guards.

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

Bitget login clock recovery at d10ba2f3c500 measures web/Node 2,782,976 B in
CI44135644635 jobs134225245837/134225246062. The coordinator-approved cap
2,793,472 B retains 10,496 B reserve at that source for family/replay checks.
M44 source7bd8f25 measures 2,784,464 B in CI44224839389/job134535952644;
artifact14393041086 has identical web/Node hashes. The coordinator-approved
2,795,520 B cap retains 11,056 B reserve, all earlier floors and strict boundaries.

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

Session adapter integration consumes native frames through `FrameStore`, including
fresh snapshot baselines on reconnect. The retained Lighter TS replay oracle uses
the generated nullable liquidation price contract: unknown prices, including
zero-quantity placeholders, are null before fixture encoding. It retains the
order-ID randomness and signature verification assertions.

#3640 includes native store projection and wire emission in every venue-family
artifact. Its final cap entries require optimized web/Node byte counts
and SHA-256 hashes from Rust 1.99.0, bindgen 0.2.129 and Binaryen 132 (two -Oz
passes), with at least 10 KiB reserve rounded up to KiB. Historical floors and
the combined ceiling remain unchanged. After M44, remeasure all families on the
merged tree and record new provenance in a follow-up commit; pre-M44 evidence
does not establish the merged composition.

The retained Bybit and inverse TS replay oracles publish unknown/nonpositive
liquidation prices as native null, including flat and option positions. Their
SCXF fixtures must pass the same generated Position contract as the Rust engine;
a zero/invalid liquidation sentinel cannot complete TS private replay startup.

BingX Spot replay tracks production decoder receipts alongside pending account reads.
Bounded drains precede ticks, checkpoints and final capture; event-loop yields
alone cannot establish completion of gzip decoding or its queued text messages.

#3640's post-M44 sweep is CI44245108129 at source
`5a8c12ca5208270288dae40df8d85f111988ea06`, tree-identical to merged head
`5a6761866990bd1894a20bc89fbedecc14b80978` (master `700dee33e3a9`).
All 27 exact family builds completed; web/Node hashes and lengths agree.
The follow-up budget commit replaces provisional `36e3733cf7` provenance with
these fresh measurements. Each non-Bybit cap is measured +10 KiB rounded to KiB.
The measured combined module is 17,860,053 B (SHA256
`eba7cc0304ceb0dbbe2ffd946c02b0975bd3ab23f8d7f09cfdd70bc08dddda5f`);
combined and Bybit ceilings remain 18,087,936 and 2,696,192 B respectively.

#3640 retained BingX Spot and OKX fixture callbacks publish SCXF through the
explicit reference producer. Aster installs its taped axios transport before
construction so its closed secondary read client inherits it as well.
GitHub runner assignment metadata may lag started measurement steps: CI44248450357
returned runner_id=0 beyond the former 60-second window. Physical-job discovery
now re-lists for at most five minutes; missing/duplicate identities, retries and
nonpositive runner IDs still refuse, and replay/signing deadlines stay unchanged.

The native workspace test row installs pinned Node and locked Yarn dependencies:
store-frames' risk and ownership tests execute maintained TypeScript/FrameStore
oracles. Missing tsx/TypeScript must fail setup rather than drop those cross-language tests.

Orderly account settings preserve simultaneous Cross/Isolated leverage: a
selected margin mode updates only that mode's positions. Unknown/global settings
retain their existing behavior. Replay TIF projection represents absence as
explicit null across order types while retaining every reported value; it must
not manufacture a TIF or hide a non-limit order's reported TIF. Known-failure
and accepted-difference manifests are unchanged by this correction.
