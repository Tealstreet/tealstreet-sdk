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

Bybit order-history retention is decided at completion and preserves earlier
page-failure causes. Fresh family and combined artifacts must retain the
10 KiB reserve.

MEXC's HTTP 425 read labels are diagnostics only; typed refusals, backoff,
signing and write outcomes retain their existing controls. Exact source
06c5b32372b7, Rust SDK run 44220118208, artifact 14390662738 measures equal
optimized web/Node/replay modules at 1,962,854 bytes. The coordinator's Rule 5
floor is 1,973,248 bytes, retaining 10,394 bytes and every historical floor.

MEXC's object-envelope guard at c2455ca0e799 measures 1,963,086 bytes in run
44255752536, artifact 14406113023. Rule 17's measured + 10 KiB rounded-up floor
is 1,974,272 bytes, retaining 11,186 bytes and all earlier measured floors.

WOO's exact signature-error auth classification changes diagnostics only; socket
recovery, signing, retry and write behavior retain their lifecycle controls.
Exact source 85a85576995b, Rust SDK run 44220127210, artifact 14390881543
measures equal optimized web/Node/replay modules at 2,683,302 bytes. The
coordinator's Rule 5 floor is 2,694,144 bytes, retaining 10,842 bytes and every
historical floor.

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

Binance saturated-history paging #3671 at exact source `80e0a8fb9fa` was
measured in Rust SDK run `44344562850`, artifact `14444829191`. Optimized
web/Node/replay modules agree at 2,282,595 bytes (SHA256
`52beaef3ebde8cfaaf777516cb2c87f0235d34a243c077adfe173a9dd5ea8ae3`).
The family floor is 2,293,760 bytes: measured + 10 KiB rounded up,
retaining the prior 2,290,688-byte floor and 11,165 bytes of reserve.
The combined artifact `14447356528` measures 17,685,652 bytes, leaving
140,140 bytes under its unchanged cap. Pinned Rust1.99.0, bindgen0.2.129,
Binaryen132 and required -Oz settings are unchanged.

#3640's final allocation/chaser sweep is CI44266062446 at source
`283acdd2f583259fa40113dc88f58b54adac610d`. All 27 exact family builds
completed with matching optimized web/Node bytes and hashes. Their native-input
fingerprint `399b3f8467a191b58ea8841ec5c9292b6533b2342ba6c1efbecdbfcc49a0a8f5`
matches branch `c0113a933d`; the complete synthetic merge tree is not asserted
identical. The follow-up refreshes bb6590f792 provenance after the native fixes.
Every non-Bybit cap is measured +10 KiB rounded to KiB, retaining historical floors.
Bybit measures 2,670,297 B at its fixed 2,696,192 B ceiling. The exact local
combined build at c0113a933d measures 17,866,681 B (paired web/Node SHA256
`0ed474e4613e860eea7c61babae81aff07fffd8aceb282e84de9282c58e87d8f`)
at its fixed 18,087,936 B ceiling. Final-head combined consumers must still run
and succeed in CI; local measurements do not grant that verdict.

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
retain their existing behavior. Orderly opts into full-order TIF comparison, representing absence as explicit
null across order types while retaining every reported value. Other families
keep their established limit-only comparison scope. The projection never
manufactures a compared TIF. Known-failure and accepted-difference manifests
are unchanged by this correction.

Native-store frame memory reservations include product metadata, ticker observation
columns, message-stat numeric rows and their string tail before temporary interning
tables are allocated. Full books reserve their known level columns, and the string
tail includes final alignment. LOCAL copies each owned flush once to JavaScript; snapshots keep independent
baselines.
The public 16-pair memory gate and all compared fields remain unchanged.

#3640 post-master rule-17 reserve audit uses CI44302321301, built source
`1fdd2db8279b2d4ab697720f2169cd0b6941a078` (the exact tree of PR head
`7e9d5723fad5e8820242a18b205c068dd8eb851f`). All 27 sealed family artifacts
have matching optimized web/Node bytes and hashes, verified source/run/input
fingerprints, and pinned tools. The complete 49-name routing and 31-name explicit
budget census found shortfalls only for ApeX, Binance, Kraken, Lighter, TxFlow and
WOO; their vetoable cap refreshes use measured +10 KiB rounded up to KiB, never
lowering a prior cap, with shared budget aliases kept equal. The budget tests
retain every family measurement and module SHA256 and require exact caps,
10 KiB reserves, alias coverage, and cap/cap+1 boundaries. Combined measures
17,869,568 B (SHA256 `b7693720dffd5c3878c09ea239e9949532c43d110e4883080aa07b476ad482a3`),
leaving 218,368 B at the unchanged 18,087,936 B ceiling. No runtime inputs change.

#3640's vetoable M46 reserve refresh uses CI44319661954 after all 31 family
measurement jobs, including shared aliases, completed successfully. Built source
`de42fedbb4021e94d1162e4420b8583d0d20906a` has the exact tree of PR head
`d931514bb9750d82e2f538ed454b0d24edd0db5f`. All 27 sealed family artifacts
pass source/run/input fingerprints, pinned Rust1.99.0/bindgen0.2.129/Binaryen132
and optimized web/Node SHA256 checks. The complete 49-name routing and 31-entry
budget census found shortfalls only for BingX, Hyperliquid, MEXC and OKX.
Their caps use measured +10 KiB rounded up to KiB, never lowering a prior cap;
shared aliases follow their parent family. Per-family bytes and hashes are pinned
in wasm-budget.test.mjs; changed caps retain all provenance comments and floors.
Combined measures 17,876,808 B, SHA256
`12320b94f86e8c4561366588bc255edc0d1b438fdcd3c8989d9e22714cc44cee`,
leaving 211,128 B at its unchanged 18,087,936 B ceiling.
This budget-only composition changes no runtime inputs; required new-head CI
and combined consumers still have to run and succeed.

#3640's vetoable M48 reserve refresh, after merging master 04a2f14e5b at
`d123c9b38c4559ab38b5df8142981c7fdcc008b5`, uses CI44389135230. Its built
source `42e792c614d768737151090feb06c4da4558855a` (GitHub's merge ref) has the
identical tree. The 27-family census found shortfalls only for Bitunix
(10,219 B), KuCoin Spot (10,212 B) and OKX (9,982 B); each moves to measured
+10 KiB rounded up. The M46 census becomes a floor and the M48 census pins the
exact caps. Combined measures 18,087,936 − 206,989 B and stays at its ceiling.

Managed HTTP preparation observations change shipped host JS, so they move the
beta input fingerprint and require a new release/pin. They do not change Rust
sources or waive fresh combined-consumer/size evidence or any retained floor.

#3671's M49 composition at `6c9370d8ecdd0e082ecb8e8942077ac011101096`
uses its own PR CI `44441524748` and the tree-identical checkout
`7d23ca64972600ed7bd9df6e690116388190e4c5`. All 27 optimized family
artifacts retain pinned tools, source/run/input seals and matching web/Node/replay
hashes. Only Binance misses the 10 KiB reserve: job `135283817066`, artifact
`14491516834` measures 2,402,182 bytes, leaving 8,314 under the inherited cap.
Measured +10 KiB rounded up gives 2,412,544 bytes, retaining the 2,410,496-byte
parent floor and 10,362 bytes of reserve. The M48 census remains a floor;
the M49 census pins every current family cap, alias and cap/cap+1 refusal.
No runtime input or other family/combined ceiling changes. Final-head required
coverage and actual native/combined consumers remain independent gates.
