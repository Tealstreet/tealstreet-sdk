# AsterDEX V1 negative joins

This family leaf uses the production Session/Registry/native-probe/remote-signer/HPKE harness over loopback. Mainnet and configured testnet each run the seven shared scenarios: genuine second credentials, wrong COSIGNED signing material, a prior signature over different full bytes, consumed reply, wrong sealed pair/secret, old/rebound seal and consumed handover.

The synthetic venue checks the native V1 query HMAC and independent key material. Token acquisition is a read-feed POST, never a trading mutation; public discovery is not private proof. Every refused join preserves the loaded resident actor, signer, owner, holdings and original registrations with zero trading writes. Wallet V3 COSIGNED shapes remain proof-unavailable before signing or private I/O. No production eligibility, classifier, catalog, codec, helper or origin is changed.

The committed unchecked-verifier checkpoint compiled: one verifier control passed and three intentionally failed; both real Session wrong-key and prior-signature cases failed at `unproved socket attached`. Wiring the independent verifier makes all controls pass without relaxing the shared permanent original-account/registration refusal. Compile/setup failures are not counted as red evidence. This is fixture-sensitivity evidence, not a production-defect claim or live venue certification.

The verifier binds the actual signed query, exactly one known API-key header, one timestamp and receive-window parameter, a fresh millisecond clock and the original query bytes. Unknown paths, order payloads and trading operations fail the fixture. Neither public discovery nor token acquisition substitutes for the signed private reads.

From the repository root:

```sh
(cd sdk-rs && cargo test --locked -j2 -p safe-cex-exchange-server --test negative_join_asterdex -- --test-threads=1)
```
