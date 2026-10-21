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
   and agent-fee reads have different subjects. Preserve unsigned public metadata.
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
