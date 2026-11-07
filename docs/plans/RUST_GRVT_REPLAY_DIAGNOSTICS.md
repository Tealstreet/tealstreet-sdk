# GRVT tape-time replay diagnostics

Baseline: actual landing `ba3d7da1d3aaba26d6c40d6615fdc467d6ec185d`.
Development branch: `test/rust-grvt-replay-diagnostics`; CI/PR branch:
`wip/harness-tape-time-grvt`. Keep frozen #1944 and the WOO
diagnostic branch separate. This is a diagnosis and possible narrow declaration
prune, not a claim that all native/reference differences are defects.

## Inventory and ownership

The existing private tape is
`sdk-rs/harness/tapes/grvt-private.jsonl`, SHA256
`6c710d7beae430bf2ddec1ef1e2a09e5c4552a91a10884e51a7c1d5b598b23ff`.
Its declaration has 33 exact keys: 17 execution identities, two caller client
identities, eight order-lifecycle fields and six missing private-stream errors
(`v1.order`, `v1.position`, `v1.fill`, each for TS and Rust).

The protocol audit accepts readiness after sending authentication. Its terminal
`errors()` method records missing coverage permanently. Establish whether an
interim capture explains the six errors or whether the final socket generation
actually remains incomplete. Preserve every real protocol error and every other
diagnostic until new evidence proves a specific declaration unobserved.

Intended files are a new focused GRVT replay-audit test, this indexed plan and,
only after proof, the exact tape-bound declaration group and nearby reporting
docs. Reuse #1944's reviewed shared final-capture API; do not create another
capture implementation or alter `venues.mjs`. GRVT login/session PRs #1905 and
#1897 are noted; no native account, signing, session, history or request-builder
file belongs to this slice. The coordinator has the concrete scope for overlap
checking before implementation.

## Controls before any prune

1. Commit this plan. Use the real GRVT socket audit with valid auth bytes, the
   actual acknowledgement and exact subaccount subscriptions. Establish a full
   working lifecycle independently of an interim capture.
2. Exercise the shared capture API with an observation before acknowledgement,
   then a complete lifecycle. Retain the original unconditional-capture REDs
   against the behavior-preserving extraction preceding #1944's correction.
   Check both TS and Rust; no substitute local audit or invented tape replies.
3. A genuinely unsubscribed late private frame must survive finalization.
   Closing one valid generation must not excuse missing coverage on a later
   active generation. Missing final streams and thrown final audits stay FAIL.
4. Obtain fresh raw replay evidence from a CI-built engine. Compare complete
   nonmetric outputs and exact failures per implementation, not only counts.
   Prune only individually absent keys on the exact current tape hash; keep
   historical groups and all remaining diagnostic bytes unchanged.

## Review and landing boundaries

Independently review the controls, causal interpretation and any per-entry raw
proof before publishing a worthwhile draft batch. Integrate actual #1944 before
final qualification; a reviewed source stack does not claim its landing. Run
fresh CI on the actual composed head. No gate, fixture, tape, CPU, profile or
production-policy edits. No merge without the coordinator's window and a fresh
separate guard followed by a head-matched merge.

## Local causal controls

Plan commit `bc050735e3d` precedes the controls. Reviewed #1944 head
`389d64e01f8` is integrated as a source prerequisite, not presented as landed.
Its helper, replay caller and audit validators remain byte-identical. The
coordinator confirmed #1905, #1897 and #1988 have no harness overlap; native
GRVT history refusal work also remains outside this slice.

The original capture module from behavior-preserving extraction `d59643b0b16`
produces exactly two assertion REDs and eight working controls. The isolated
probe changes only the venue-module import location; the original module bytes
are exact. Each failed healthy-final assertion contains precisely the three
missing streams recorded by the earlier checkpoint observation.

With the existing shared final-capture correction, all ten new controls and
14 existing GRVT controls pass. Genuine pre-ACK private data, a late wrong
subaccount selector and an incomplete replacement generation remain failures.
Both implementation labels exercise the same protocol-audit implementation;
this is a capture-contract proof, not native/reference financial parity.
Independent control review: `/tmp/codex-grvt-replay-controls-review.json`,
SHA256 `0a9dd80af055efcf8366972235e66c5a51dc5e73183123c0f962fade74a0f0ca`.
Declaration changes still require the separate raw evidence step.

## Exact declaration evidence and batch

Baseline #1902 run `40037292055` / artifact `12537971676` and candidate
#1944 run `40040892521` / artifact `12538743583` contain the same private tape.
Both archive lengths/digests, physical producer uploads and measured checkout
parents/trees are authenticated. Complete TS and Rust raw objects are equal
after omitting only top-level metrics and protocol errors. The baseline has
three TS and six Rust protocol occurrences, covering six unique declared keys;
the candidate has none. The other 27 ordered failures, fixers and strict FAIL
are unchanged. Unserved, missing-write, invalid-signature and unused-write counters are zero,
with six writes and nine checkpoints per receiver.

Independent exact raw review is CLEAR:
`/tmp/codex-grvt-replay-evidence/independent-review.json`, SHA256
`0b1de5f1261505a046f5cd10647a02f5ea4b6f1f56e16b3f4a7bbf2e0f5d1b10`.
Remove only that current tape's six-key protocol group; every other manifest
byte is preserved, including the WOO prerequisite, historical and public cases.
The real suite verifier retains one public PASS and one declared private FAIL,
with zero unexpected or unobserved declarations. All 32 focused controls pass.

The first prune attempt correctly stopped on an outdated review-receipt hash
while the reviewer finished sealing it; no declaration changed in that attempt.
The final sealed review above authorizes the applied exact prune.

Evidence currently contains one correctness pair per role, not five-pair
determinism. The original sweep branch from merged #1646 is fast-forwarded to
this work; its existing workflow selects five private pairs without a workflow
edit. The draft requires its fresh CI, complete per-implementation repeat check
and actual #1944 integration before final qualification. No merge is requested
by preparing this batch.
