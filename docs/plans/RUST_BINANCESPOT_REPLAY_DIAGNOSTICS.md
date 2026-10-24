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
