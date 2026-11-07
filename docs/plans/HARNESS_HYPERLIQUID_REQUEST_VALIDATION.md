# Hyperliquid independent request validation

Status: active harness plan after the completed tape-time sweep (#1694).
Landing: feat/rust-rewrite. This phase addresses the independent request/signature
gap for Hyperliquid perp and its embedded Spot account replay. It does not turn
existing declared account-state failures into parity passes. Standalone Spot's
existing verifier and production SDK/Rust signing stay intact.

## Plan

1. Add a harness-owned validator factory on the Hyperliquid catalog. Tape already
   invokes it on original requests before canonicalization, response matching or
   ACK consumption. Independently reconstruct the L1 action hash from original
   ordered msgpack bytes, nonce and vault/expiry suffix, then recover the EIP712
   Agent signer with generic crypto primitives. Do not call production signing
   helpers or derive expected values from the candidate output. Bind configured
   endpoint/network, public signer, vault and private info subjects; account reads
   and role-discovery reads have different subjects; userFees belongs to the
   trading account, not its signing agent. Preserve unsigned public metadata.
   Keep crypto loading lazy to preserve public/performance cold-load workloads.
2. Validate fields that normalization would otherwise erase: signature/nonce,
   generated cloid representation/uniqueness and caller-ID ownership. Preserve
   existing economic request matching, ACK stages, builder/core/HIP-3/embedded
   Spot wires and fixture bytes. Use the existing public official SDK key only.
   Treat unsupported signing envelopes explicitly; never silently omit a signed
   expiry field or accept a wrong vault/network with a valid signature.
3. Prove the verifier against published mainnet/testnet/vault signing vectors
   and offline negative controls: wrong signer/domain/network, action or nonce
   tampering, malformed/stale nonce/signature, wrong vault/account/fee subject,
   malformed/duplicated generated cloids. Exercise actual Tape.respond to prove
   rejection occurs before request identity normalization or ACK consumption.
   Update direct Hyperliquid clock tests to pass actual fixture configuration
   and valid signed wire requests instead of weakening the verifier for their
   placeholder fixture signatures. Preserve their causality assertions.
4. Review the plan and implementation independently. Run focused controls and
   appropriate harness checks with Node22/corepack yarn. Generate unchanged
   private fixtures and require current-head CI five-pair evidence for both
   perp/embedded Spot, all raw outputs/IDs/receipts/checkpoints, strict mandatory
   cases and exact tape-bound declarations. Inspect any new difference rather
   than widening policy. No local release wasm or public timing sweeps; reuse
   authentic matching CI artifacts only with the hard fingerprint/no fallback.
5. Integrate actual landing dependencies, preserve all incoming files, require
   full applicable CI and independent current raw review, then run the fresh
   fail-closed guard and merge in separate calls. Audit/report actual merge
   parents/files. Add this plan to docs/plans/README.md once actual #1696's index
   is integrated. Next phase is Orderly ACK fencing, then only demonstrated
   oracle/recording defects from #1696. Report items requiring live captures;
   do not fabricate data or change native fill provenance/contracts.

The fresh branch reuses the existing Hyperliquid sweep CI name so only this
matrix job collects five private pairs; the completed local branch is archived
under wip/harness-tape-time-hyperliquid-completed-1689. No history rewrite or CI
default change is required.

## Protocol sources and evidence boundary

- Official signing guidance: https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/signing
- Official independent algorithm/vectors: https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/master/hyperliquid/utils/signing.py
  and https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/master/tests/signing_test.py
- Local production implementations are comparison subjects, not the oracle:
  hyperliquid.signing.ts and sdk-rs/venues/hyperliquid/src/sign.rs.
- Signature/request validation establishes only the observed request contract.
  Existing strict account differences, fixture representativeness, missing
  native metadata and live-environment evidence retain their separate limits.

## Implementation and validation

The factory validates original wire requests before canonicalization. A lazy
harness-only oracle recovers the configured signer from ordered msgpack/EIP712
bytes, binds mainnet/testnet, vault and private account/agent read subjects, and
checks the documented open nonce interval plus the highest-100 signer nonce
replay window. This is a fresh offline signer stream, not unseen shared-wallet
history from live processes. Optional null expiry is treated as absent, as in
the official SDK. Published independent main/test/vault
vectors and an order connection ID guard against shared signer mistakes. Safe
integers beyond msgpack's 32-bit forms and lossless JSON uint64 order IDs retain
their integer wire encoding; field ordering remains signature-sensitive.

Creation cloids are required and unique. Caller IDs stay exact; generated IDs
bind by the original queued action's order position and economic key. Later
modify.order.c references must use the acknowledged alias (including
optional-ID presence). The original queued body, rather than its normalized key,
is the oracle. Signed missing/wrong references reject without consuming ACKs.
Documented cancelByCloid {asset, cloid} and modify.oid matching stay exact; this phase adds no alias normalization
for generated cloid amend targets. Unsupported remapped targets fail closed.

Tape accepts a state-commit callback from a validator and runs it only inside
Responses' matched-record hook after command/time eligibility. Premature staged
or timed requests bind neither creation IDs nor aliases. Identical-intent retries
within a command remain allowed; later commands cannot reuse a creation ID.
Standalone Spot, production signers, the comparator, declarations and fixture
bytes remain unchanged. Request validation certifies the exercised authentication
and intent contract, not full action-schema coverage, private parity or live
behavior. Existing six/eight private account differences remain strict FAILs.

Focused protocol/clock tests: 30 pass. Removing only the catalog factory makes
the actual Tape invalid-signature regression fail with "Missing expected
exception": the old path consumed the ACK. Cross-action nonce reuse, eviction/out-of-order values, time-stage poisoning, missing/changed
references, caller ownership and uint64 tamper controls pass with the factory.
Initial full serial harness: 1037 pass; protocol-shape follow-up checked separately. Current-head CI/five-pair proof remain required before
merge; final run IDs/hashes belong in the PR body.

Initial CI39695664088 passed all1038 harness tests but the public TS replay
rejected its recorded anonymous userFees probe. The live public fixture already
contains exactly userFees/user="" with HTTP422. Preserve only that explicit
public-data-only, no-wallet rejection/fallback: matched status must remain422;
a fabricated200 and every private empty-subject read reject. This is not an
authenticated account read. One diagnostic TS correctness replay identified the
failure; no local public timing or release build was run.

Late independent-source audit corrected an incorrect agent-fee assumption before
merge. Official info-endpoint rules bind userFees to the trading account; only
userRole uses the signing key for this connector's discovery flow. Distinct
signer/master/vault controls now reject signer-based fee queries. The equal-key
private fixtures do not expose this configuration gap and their bytes stay
unchanged. Both production implementations still select options.key for fees
(Rust account.rs:200-203; TS hyperliquid.exchange.ts:1465-1466); reported separately,
with no production fix in this harness PR. CI39696883317 evidence is historical
for the pre-correction head; current-head CI/proof must refresh before merge.
Sources: https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint
and https://hyperliquid.gitbook.io/hyperliquid-docs/trading/fees .
