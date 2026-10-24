# Binance Spot tape-time replay diagnostics

Baseline: actual landing `ba3d7da1d3aaba26d6c40d6615fdc467d6ec185d`.
Development branch: `test/rust-binancespot-replay-diagnostics`.

## Selection and overlap

Binance Spot has four untouched protocol keys after the parked WOO, GRVT and
Crypto.com sweeps: TS/Rust subscription and request acknowledgement coverage.
The current tape is `sdk-rs/harness/tapes/binancespot-private.jsonl`, hash
`17f14a131f0240857e7a0866432587f8929a9aea084d1f619c65a88bb4309c67`.
Its other 28 declared keys cover fill prices, time in force, caller identities
and order lifecycle; they remain strict failures unless separately proved.

The refreshed map includes all 82 open PRs and both complete paginated file
lists. No PR touches the Spot audit/generator, existing protocol test, proposed
new audit test or shared venue registry. #1944, #1998 and #2002 overlap only in
shared capture/manifest paths and preserve the complete Spot declaration case.
Receipt: `/tmp/codex-binancespot-replay-overlap.json`. Native Binance source,
listen-key/session work and other queued family adoptions stay outside scope.

## Plan and controls

1. Commit this plan before tests. Exercise the actual audit using independently
   computed signed subscription HMAC, exact typed request IDs, subscription ACK,
   matching heartbeat reply and subscription-scoped private events.
2. Reproduce the original sticky coverage errors by observing a checkpoint
   before the subscription ACK, then completing a healthy lifecycle. Assert the
   final result first against the exact historical unconditional capture module.
   Reuse the reviewed #1944 terminal-capture helper only if needed; never stack
   unrelated GRVT/Crypto.com work or copy the capture policy.
3. Keep working complete-lifecycle controls and genuine invalid signatures,
   mismatched ID types, pre-ACK events and final unanswered requests visible.
   Pin actual connected/closed generation semantics with an incomplete active
   replacement and a healthy replacement of a closed pending generation.
   Final errors must be captured before memory settling can deliver a late ACK.
4. Authenticate fresh source-bound complete TS/Rust raw evidence for the exact
   tape. Compare all fields except top-level metrics/protocolErrors; preserve
   ordered remaining failures and fixers. Prune only individually absent keys
   after independent review, with every other manifest byte unchanged.

## Publication and qualification

Own scope: a new audit test, indexed plan, exact proved declaration group and
nearby reporting docs. No native source, audit validator, request/signing/socket
policy, tape/fixture, build/profile, CPU threshold or workflow implementation
change. Publish a reviewed worthwhile batch as a draft. The existing matching
sweep branch selects five private pairs without changing workflow policy.
One historical pair per role is bounded evidence, not repeat qualification.
Actual #1944 integration and fresh composed five-pair CI are required before
final qualification. No merge without a granted window and separate guard.

## Local regression controls

The exact historical unconditional capture at `d59643b0b16` has two assertion
REDs with fourteen working controls. Each RED asserts the completed final result
first: a pre-ACK checkpoint permanently records both missing subscription and
request acknowledgements in that receiver. All sixteen controls pass with the
reviewed #1944 helper. Invalid HMAC, wrong typed ACK IDs, pre-ACK private data,
active replacement incompleteness and a genuinely unanswered final request stay
visible. Closed pending generations are ignored only through the existing
explicit close/connected contract. Original/corrected logs are retained under
`/tmp/codex-binancespot-original-capture-controls/`.

## Source-bound evidence and integration

The authenticated strict Binance family archives are baseline #1902 run
`40037292055` / artifact `12538128543` and corrected #1944 run `40040892521` /
artifact `12539504611`. Their complete private Spot objects differ only in
metrics and protocol errors: TS two and Rust two acknowledgement errors become
empty arrays. All twenty-eight other ordered failures and fixers remain exact,
with three writes, six checkpoints and zero unserved/missing-write/signature/
unused-write counters in both receivers. The source-bound private status stays
FAIL. Spot's public case is PASS. Both archive digests/lengths, producer upload
logs/windows and checkout parents/whole trees were independently authenticated.
These archives use the family name `strict-replay-binance`; the proof selects
only the exact private Spot tape from the family's nine cases.

Raw proof: `/tmp/codex-binancespot-replay-evidence/proof.json`, SHA256
`366eb4a3afe6d07ee75bb925b83e20a9bd08a37e61fd8816925a65bca4ffa8fe`.
Independent raw review: `/tmp/codex-binancespot-replay-evidence/independent-review.json`,
SHA256 `281e0b6c7fd740f36e208dcaa556ca45c909b158c548d06abee7028ce839a512`.
Independent controls: `/tmp/codex-binancespot-controls-review.json`, SHA256
`b381c7daf54fbbc74f02cd34f868c417a56fd6efc7a9832e9e5df836fbdf1b0c`.

Actual #1944 `10816d4ab1f14e13a201936dacec7ea237be9bf9` is genuinely integrated
at `10938987b19`. Before pruning, all incoming source and shared capture bytes
were preserved, with only the indexed plan and new control file added; the
existing protocol/declaration controls passed 42/42. Only the 492-byte four-key
Spot group is subsequently deleted. Every other manifest byte remains exact.
The real complete-family verifier has four strict PASSes and five declared
strict FAILs, zero unexpected failures and zero unobserved declarations.
Fresh current-head CI must still provide five private pairs through the existing
`wip/harness-tape-time-binance` branch policy; no workflow change is needed.
