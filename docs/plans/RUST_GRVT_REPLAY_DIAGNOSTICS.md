# GRVT tape-time replay diagnostics

Baseline: actual landing `ba3d7da1d3aaba26d6c40d6615fdc467d6ec185d`.
Branch: `test/rust-grvt-replay-diagnostics`. Keep frozen #1944 and the WOO
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
