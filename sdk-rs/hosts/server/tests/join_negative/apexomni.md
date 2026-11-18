# ApeX Omni negative joins

This leaf uses the real Session/Registry/native probe/remote signer/HPKE paths over loopback. Mainnet and configured testnet each run the seven shared credential, replay, consumed-response and sealed-handover scenarios. Refusals retain permanent original-account/instance binding, the loaded actor/provider/signature/owner/fingerprint/holdings/registrations and liveness, with zero trading writes.

The independent synthetic venue uses ApeX's base64 TEXT of the raw API secret as its HMAC key. REST binds the actual timestamp/method/path with singular headers; private WebSocket login binds its native JSON-string argument and millisecond timestamp. Login state belongs to each socket. The actual atomic `apexomni-batch` descriptor carries read/login bytes with no order contracts; its reply returns credential and MAC, never passphrase or secret. Public symbols are not private proof. This proves API-credential possession, not OmniKey/L2-seed possession or placement capability. No production/helper/catalog/origin/codec/eligibility change.

The committed unchecked-verifier checkpoints compiled with two controls passing and three intentionally failing; both real Session wrong-key and prior-signature cases failed at `unproved socket attached`. The refined replay checkpoint reuses only prior private REST signatures over different full bytes and answers public/login prerequisites freshly. Separate wire controls reject wrong/rebound WebSocket login MACs. No compile/setup failure is counted as security evidence.

A first candidate also exposed a fixture lifecycle mistake: native ApeX pipelines private subscriptions before the login ACK, while the fake initially panicked on an unproved subscription and poisoned the observation lock. The focused committed regression fails before the correction. The final fake returns an explicit authentication refusal without private data, preserving per-socket login isolation and all shared permanent/resident assertions. Those former timeouts are not security rejections.

Omitting the OmniKey seed retains the native monitoring diagnostic and placement refusal; the suite does not invent a seed, silence the warning or claim L2 signing capability. No live venue certification.

From the repository root:

```sh
(cd sdk-rs && cargo test --locked -j2 -p safe-cex-exchange-server --test negative_join_apexomni -- --test-threads=1)
```
