# Orderly command ACK fences

Status: active harness phase after Hyperliquid validator #1698
(`6b2c9c9b3e3`). Landing: feat/rust-rewrite. Fresh branch from that actual
landing; the completed local Orderly sweep branch is archived without rewriting
history. No production Rust/TS, signing, routing or economic payload changes.

## Demonstrated gap and plan

1. `private-tape-orderly.mjs` stages GET order snapshots by command but leaves
   mutation ACK records unstaged. `Tape.readStage()` consequently exposes a
   current command's final snapshot before its write is acknowledged. Add only
   `afterCommand: commandCount` to mutation HTTP records in the shared generator.
   Keep existing read stages, all wire payloads, times, ordinals, identities and
   the fourteen broker/network combinations unchanged.
2. Commands 3–6 each require two writes (cancel/replacement, child replacement,
   bracket creation and bulk cancellation). The current first-ACK release is
   insufficient for their final snapshots. Add an opt-in `waitForAllStageWrites`
   catalog flag to the shared Orderly factory and pending counts for its staged
   mutations. Release the read fence only after every eligible matched ACK for
   that stage; rejected/unmatched/599 or repeated attempts must not decrement it.
   Preserve other venues' existing behavior and independent `beforeAck` event
   delivery. Keep the existing settled-command fallback: settlement is a replay
   scheduling boundary, not proof that every write succeeded.
3. Add offline adversarial tests across seven brokers and both networks. Use
   actual Tape matching and valid signatures from the existing public replay
   credentials. Advance elapsed time beyond every future snapshot, then check
   ordinary/algo reads before every ACK, between multi-write ACKs, after all ACKs
   and before the next command. Include amend price and algo-child identities,
   not just row counts. Cover wrong-stage, invalid/unmatched/duplicate requests,
   nonmutation commands, settled fallback and unchanged first-ACK behavior for
   non-opt-in venues. Preserve existing before-ACK controls.
4. Prove fixture identity by stripping ONLY the newly added mutation-record
   stages; existing GET stages must remain. The resulting fourteen byte streams
   must reproduce their original SHAs and all 82 records/14 commands. Bind only
   the fourteen affected active manifest entries to new SHAs, retaining all keys
   and reasons initially. Prune only exactly unobserved leaves proven by the
   complete fresh proof; new differences require independent cause review and
   exact rationale, never automatic allowances or normalization changes.
5. Independently audit plan and implementation; prove regressions fail with
   the original generator and with first-ACK release. Run focused and serial
   harness checks using Node22/corepack yarn. Integrate actual source/build
   dependencies (including pending Hyperliquid fee fix #1701 if it lands),
   preserving every incoming/owned blob. Use matching authentic CI engines with
   the hard fingerprint/no fallback, or own-head CI. No local release builds,
   public timing or full replay sweeps. Existing branch-specific five-run mode
   covers all fourteen private calls; ordinary CI remains one run.
6. Require all 140 private outputs complete and identical within each
   implementation/tape except top-level measured metrics, preserving full IDs,
   receipts and checkpoints. Verify all fifteen mandatory suite cases and
   exact declarations, retain strict known FAILs, get independent current raw
   review and full green CI. Guard and merge in separate calls, audit actual
   parents/files and report the merge before the next recorder phase.

## Documentation additions and evidence limits

- Add this plan to the shared README index. Update the Orderly ACK caveat in
  HARNESS_TAPE_TIME and the architecture gate audit only to the extent proven.
  Deterministic/authenticated scripted replay does not certify private parity,
  live recordings, native missing timestamps or cross-process nonce history.
- Overseer explicitly requested two known Hyperliquid nonce limits in the
  RUST_AUDITS Hyperliquid section as this follow-up's deliberate docs addition:
  static signer history survives engine recreation within one process/Wasm
  memory, but a same-ms fresh process/module restart with a counter ahead of
  wall time may reuse history; and a >1-day forward clock excursion or sustained
  >1000 writes/s per signer can exceed the venue future window because the
  allocator checks safe-integer bounds rather than venue freshness. Cite
  crypto/src/nonce.rs:1–2,9,19–28,33–44; HL trading.rs:641–644; SDK
  rust.exchange.ts:892,917 and virtualClock.ts:98–110,151. The owner chose no
  production fix: these are not #1693 regressions and synchronized time helps.
- The bounded ts-oracle/fixer-contracts/legacy-fill review found no additional
  demonstrated circularity or overbroad reconstruction defect. Retain executable
  TS references and current reconstruction limitations. Next inspect offline TS
  recorder support; actual new public/private captures, shadow/canary and native
  provenance/contract work stay skipped and explicitly reported.

## Verification in progress

The plan was reviewed before implementation. The focused suite has 35 passing
controls. Removing the mutation stages reproduces premature order 30 visibility
before the first ACK; restoring first-ACK release exposes algo 200/children
201–202 before command 3's second ACK. Both negative controls fail the causal
regression, and the restored implementation passes. The implementation review
found no remaining fence bypass. Original byte identity covers all fourteen
82-record/14-command/12-mutation fixtures; manifest changes initially substitute
only their fourteen SHAs, preserving every existing diagnostic key and reason.

Actual fee-subject fix #1701 (`b1510f267c78`) is integrated with every incoming
file and existing harness edit preserved. Its authentic Orderly engine from
CI `39699465571` passes the hard build-tree fingerprint
`f9d397f2050b2c79ff4a8b26322c6ba77d55d699d0ada8cd4176f6bc375fb046`.
Local private iteration completed all 140 outputs: five identical full
nonmetric outputs per implementation/tape, full IDs/receipts/checkpoints retained.
All fourteen cases retain exactly 912 observed declarations with zero unexpected
or unused keys, so no pruning or new allowance is required. This remains
preliminary evidence: full PR-head CI, all fifteen mandatory cases and independent
review of the current CI artifacts still gate merging.

Local verification: all 1,077 serial harness tests, 44 root typecheck tasks and
28 root lint tasks pass. Independent raw review confirms complete IDs, 14
command results, 12 writes and zero signature/protocol/missing-write failures in
each case. The complete nonmetric outputs equal earlier #1692 CI artifacts: the
repair changes eligibility under adversarial timing without changing these
observed outputs. Fresh current-head CI evidence remains required.
