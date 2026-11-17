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

BingX Spot qualification adds independent Spot public/private cases to the `bingx` suite.
The admission candidate replays Spot through the normal constructor in the BingX family.
The nondefault `bingx-spot-replay` feature retains a fixed-credential fixture export,
but production parity does not select it. Live smoke and release remain separate gates.
Shipped family size checks run before the optional fixture build.
The ordinary BingX family includes the existing book and symbol-cancel input
codecs. Offline production probes run through the shipped/fixture admission
entry point and require no dispatch of unadvertised commands; perpetual capability
flags and the Spot admission/smoke gates remain independent.

WASM venue caps retain both parents' measured floors. Fresh compositions must
retain at least 10 KiB above their optimized web/Node measurement, rounded up
to KiB; boundary tests retain inclusive cap and cap+1 refusal checks.

#3599 links `Engine<BingxSpot>` under Sam's 2026-11-17 decision (h). Canonical
combined web/Node at `657903ad6b2095ed7145c105f847ed179a50210e` measure 17,909,819 B;
its historical 17,921,024 B ceiling retained 11,205 B reserve and the 17 MiB floor.
Future combined changes require a fresh optimized measurement plus at least
10 KiB rounded up to KiB; required coverage and executed combined consumers remain gates.

The clean e009 integration has the same tree as CI bdc55d20 in run 44089335343,
job 134060777063: optimized combined web/Node 17,917,521 B. Its 17,928,192 B
ceiling retains 10,671 B reserve and both earlier combined floors. Native
REST-book conformance uses mainnet only for BingX Spot, which has no testnet;
requests are captured without transport and testnet Unsupported/NotSent remains tested.

Fresh canonical strix builds at `44f9dedd` measure combined web/Node 17,918,873 B,
1,352 B above that separate CI measurement. The approved 17,929,216 B ceiling
uses the maximum plus 10 KiB rounded up to KiB, retains 10,343 B reserve and
all prior floors. The BingX family measures 2,411,142 B; its unchanged
2,421,760 B cap retains 10,618 B. Neither measurement substitutes for final-head CI.

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
