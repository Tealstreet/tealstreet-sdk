# Kraken Futures public candle custody

Plan-first base: actual `10d01de5279`. Branch:
`feat/rust-kraken-futures-candle-context`. No combo-5b source or key-selection
changes are included. Kraken owner codex-hcmq3f released the candle regions;
open #1876 owns managed-write plan/tests and #1798 owns Spot book/CRC32 paths.
Neither intersects this slice. Preserve actual #2000 discovery/local custody and
#2014 fixed settings; do not compose their obsolete branches.

## Exact inventory and source boundary

Frozen checker inventory has Kraken27 occurrences/23 fingerprints. Futures
lib.rs has four occurrences/three fingerprints, all belonging to candle work:

| Native site | Count |
| --- | ---: |
| candles_page: cx.request_streamed | 1 |
| public_failed: numeric command failure | 1 |
| final candle response: numeric done/fail | 2 |

Intended shrink is family27->23 and lib.rs4->0. No manifest changes until actual
production adoption; then prune only this exact key. Remaining account/signing,
private history, trading and test inventory is outside scope. The raw streaming
funnel cannot be renamed or hidden behind another helper to claim a prune.

Source scope is Futures Pending::Candles and its request/page/stream/response/
failure ownership, plus root retirement if needed. Add an isolated native test
`tests/futures_request_context_candles.rs`. Keep Spot, account.rs, history.rs,
trading.rs, sign.rs, secondary tests, wire codecs, source key choice and shared
engine/registry/crypto code unchanged. Existing native assertions stay intact.

## Native geometry and behavior

Command FetchCandles and subscription polls share an unsigned GET chart route:
`/api/charts/v1/trade/{symbol}/{resolution}?from={floor(ms/1000)}&to={floor(ms/1000)}`.
Headers remain empty, body absent, Public egress, no credential or signer.
Unknown market, unsupported interval and invalid range keep their existing local
Read behavior and messages. Preserve read-only validation ordering; issue no
second root after those already-managed local terminals.

Preserve default limit500 and clamp1..5000 for deriving the default window;
explicit ranges are not locally truncated by limit. Resolution, row numbers,
ms-to-seconds conversion, response-envelope validity, sorting and duplicate
winner/order remain native. The next page starts at the last row timestamp in
ms plus the interval, strictly advances, stays <=to and obeys ten pages total.
A more_candles response with no viable cursor or exhausted budget fails the
whole command; staged pages cannot become a successful prefix. No new retry.

Background subscriptions keep their existing generation/subscriber/in_flight
checks and native five-second poll/backoff. Only a current generation may emit
a candle. A stale success OR failure cannot change a recreated subscription.
Retain the native latest-bar/change condition and no command terminal for polls.

## Existing shared authority, without 5b

A valid command walk admits exactly one original caller-wait Read root with
`admit_caller_read_root(None,None)` in genuine Fresh entry, after the existing
read-only local validation and before remote work or result mutation. It keeps
the exact raw caller receipt deadline, with no guessed root timeout. Rejected
already emitted one refusal; Revoked uses the existing expired-command helper
only while genuine entry remains Fresh. Missing receipt stays the shared typed
refusal, with no default timeout or numeric completion fallback.

At each eligible command page dequeue derive a fresh Read/Never physical child
from that ORIGINAL root via http_leg_context, timeout20000ms, no credential,
no pacing. Child deadlines cap at the original root; pages never derive from a
previous child or renew the root. Queue through request_managed and retain
stream_response(ArrayAt::Field("candles")); exact root+leg delivery is checked
before every streamed row, native envelope handling and result publication.
Only original-root done_context/fail_context terminates the command.

The explicit20000ms admission-to-expiry child/background policy is a NEW caller
contract: native candles_page emits HttpRequest.timeout_ms=None and makes no
request_timeout declaration. The legacy engine/host default is not a native
whole-admission bound. The new bound includes stuck Prepare/Bind time; CALLERS
must state that change separately from the unchanged native wire/paging/polling.
Each background page uses its own Background Read Context with this explicit
20000ms/Never/None/no-pacing policy, without a20s logical deadline across pages.
Validate exact background HTTP delivery plus subscription generation before
staging/publishing, and finish that physical Context once. Preserve next_poll
and in_flight cleanup on every queue/preparation/network/timeout/native failure.
No command root, Plan, Session or selected-key wrapper is fabricated for polls.

Retire completed physical contexts with finish_http_context, never a child
command terminal. Read DATA validation does not make a codec-valid candle.
Queue/preparation errors remain typed and strict, never transport retry
evidence. Keep native Warn diagnostics and text alongside the typed Read failure.
Preserve every shared typed failure field: pre-release denial/timeout can carry
local NotSent under the shared1870 projection. Initial admission, released Read
failure and original-root expiry keep their existing shared projections; no
financial write fact is fabricated, and no blanket outcome stripping is allowed. Command root expiry is engine-owned strict
Timeout: discard all staged pages, no candle/result/marker/continuation and no
local stop flag or root renewal. Pending command rows are owned only by the
retired root; use the existing retirement hook if any additional native parked
work is discovered. No shared authority/lifecycle edits are authorized here.

## Original controls before source

Compile and execute against unchanged10d production; count only assertion
failures whose setup and working controls succeed. A real thin Venue probe can
retain genuine Command Read peers for capacity and retire exact owners; it may
forward native callbacks and seed normal fixture data, never construct Context,
Attempt, permit or provider success. Managed lifecycle controls must traverse
real Prepare/Bind/Claim, and use the same harness on native raw HTTP for working
wire/codec controls. Missing future preparation stops that test at its first
assertion; do not claim suffix execution.

Cover command and background prepare-before-I/O, genuine capacity/refusal and
recovery, typed route preparation refusal/late success, original fractional
caller cap, live-root physical timeout versus strict original-root expiry,
second-page root retention and expiry with staged rows, and current-generation
publication only. Pin native single/multi-page wire/codec/defaults, ten-page and
no-progress failure, sort/dedup, five-second repoll, unknown/unsupported/range
precedence and unsubscribe/recreate behavior. Duplicate command IDs, unstamped
entry and invalid event clocks are separate shared-boundary controls.

Plan commit precedes the new target; original assertion checkpoint precedes any
production. Ask codex-hcmq3f for native fit and the root owner for bounded API fit;
production waits for those reviews and the original-control review. Overseer
approved REDs, review, production and parallel CI. Local cargo is only this
crate/target with CARGO_BUILD_JOBS=4, one build at a time; no workspace/all-feature
sweep. Full CI owns WASM, replay and workspace proof. Do not touch frozen2033.

## Original execution checkpoint

Native/geometry fit is CLEAR from codex-hcmq3f at4d8b97cd79a; fe7lgm root/API
fit is CLEAR with the same new-policy and shared-metadata declarations.
On unchanged10d production, the new target compiles and executes25 tests:
**14 intended assertion REDs, 11 working GREENs, zero ignored**.

The RED first assertions expose raw HTTP before preparation (command, background,
second page, route-refusal and held-permit controls), capacity bypass for command
and poll, duplicate command reissue, invalid-clock HTTP, late success beyond the
original caller wait (including a staged first page), expired background candle
publication, and absent caller/physical deadline timers. The deadline assertions
are NEW-CONTRACT custody/policy specifications; native raw packets declare no
request_timeout, so they are not mislabeled as an existing20s native policy bug.

Preparation/Bind/Claim, typed pre-release localNotSent, late-result silence,
capacity recovery and peer-preservation suffixes remain gated by their first RED.
No executed proof of those future suffixes is claimed. The11 working controls
pin unsigned wire and numeric conversion, default-window clamping, explicit-range
limit behavior, native sort/duplicate winner, multi-page continuation, strict
no-progress/later-page failure, ten-page bound, empty success, native five-second
polling/current generation, local validation precedence and zero-wait entry.
Shared missing-receipt behavior remains core coverage; no synthetic native
unstamped-entry seam or fabricated Context was introduced for this checkpoint.

Initial compilation used floating waits instead of the API's u32 and is excluded
from the RED receipt. A fixture initially chose a socket supervisor's five-second
wake instead of the first native poll timer; its corrected working control now
passes. Those setup failures are not adoption defect evidence. Source/maps/Cargo
and existing tests across219 Kraken/engine/crypto/registry/checker paths remain
byte-exact10d. Scoped target strict Clippy and rustfmt/diff checks pass with
CARGO_BUILD_JOBS=4 and sequential builds. No full-workspace/all-feature build.

## Root-owner original-control review fold

Fe7lgm reviewed9afcaf83f17 read-only: original assertion/custody fit CLEAR with
a required coverage gap. The held-bind original used a1s root and therefore
proved root expiry, not the NEW20s admission deadline under a longer live root.
Two appended controls now hold Prepare unanswered or Bind unclaimed under a60s
original root, fire the genuine20s child timer and require exactly one Timeout
with the existing localNotSent projection, no HTTP/next page, and inert late
preparation/claim. These controls first fail at raw HTTP on unchanged production;
the timer/metadata/late suffixes remain gated, not already executed proof.

A third appended control accepts page1, releases page2, then requires physical
child Timeout with no prefix/third page/late result. Its original first RED is
the absent child deadline. The existing staged-root-expiry control additionally
pins no HTTP or Prepare after its first failed terminal assertion. Primary
original boundaries and all native assertions are unchanged. Current target28
executes **17 intended assertion REDs / 11 GREEN / zero ignored** against10d.
The final review delta requires the root owner's bounded closure before source.

## Source-stage capacity suffix correction

Fe7lgm cleared the28 original controls at f0f8f9711dd for source. The first
implementation run passed27/28 and exposed one previously gated fixture
assumption: a command walk needs TWO registry Context slots (original root and
one physical child), whereas the suffix freed only one of256 peers. Original
initial-capacity assertions remain unchanged. After one retirement the root
admission succeeds, the first child correctly refuses Capacity, and the command
seals once with typed Internal/localNotSent, no command_error, HTTP or Prepare.
Every255 retained peer validates. Retire exactly one additional peer, validate
all254, then a fresh command succeeds: this also proves the refused root slot
was released. Root admission reserves its own slot only; there is no new shared
reservation or engine bypass. The root owner cleared this honest suffix fix.

## Implementation checkpoint

All28 controls now pass without ignored tests, including the corrected capacity
suffix. Existing public17, public_local_context13, socket_supervision15 and
startup_resilience2 pass unchanged (47 total). Source change is confined to
Futures lib.rs candle ownership/callbacks, preserving foreign typed callback
defaults. Physical contexts retire between pages; background publication occurs
synchronously after exact delivery/generation validation and before retirement.
Only the lib.rs manifest block (four occurrences/three fingerprints) is pruned.
No shared, Spot, private signing, existing fixture or Cargo edit. Source reviews
and CI are requested separately; no landing/ready claim.

Scoped strict Clippy passes after the capacity correction. The15 request checker
controls and8 selection controls pass. Bans/ratchet currently reports only
incoming prunes since10d (Backpack trading, Crypto.com trading, WEEX depth/lib);
those four sources and manifest sections are unchanged from10d. Integrate actual
landing to inherit those prunes before CI; do not edit their inventory manually.

## CI unit-fixture compatibility correction

Exact4371177c6cf received independent core/source CLEAR from uufub4 and fe7lgm
(the latter executed28/28), plus hcmq3f native/source/caller/map CLEAR read-only.
Full Rust40096740976 then found a genuine unit-test compile error: the native
secondary_tests probe_pending carrier still initialized the removed command
field. Its sole constructor change is command:None -> root:None/context:None.
The ProbeVenue ignores responses; no Context is manufactured and all signing,
selection, request and assertion text remains unchanged. Production and all28
new controls remain byte-exact437. This compatibility carry adds secondary_tests
to the owned test paths, without another manifest delta. Local unit+dedicated
execution and scoped all-target strict checks replace the earlier incomplete
lib-only Clippy coverage; fresh CI and a bounded delta review follow the fix.
