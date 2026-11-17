# Binance negative joins

`negative_join_binance.rs` runs the reusable real Session/Registry/probe/HPKE
suite with an independent synthetic Binance venue. The shared harness belongs
to the parent PR; this leaf changes only its own adapter, runner and this note.

| Exact name | Offline scope |
| --- | --- |
| binance | USD-M mainnet and configured testnet |
| binancepm | Portfolio Margin mainnet and configured testnet |
| binanceinverse | COIN-M mainnet and configured testnet |
| binancespot | Spot mainnet and configured testnet |
| binanceoptions | Mainnet; testnet refuses in COSIGNED and DELEGATED |

These are loopback tests of configured native behavior, not live venue or
sandbox availability certification. Each admitted scope exercises an independent
second valid pair, wrong COSIGNED key, previous-attempt signature, consumed reply,
two wrong sealed credential shapes, old/rebound handover and consumed handover.
The shared resident assertion preserves the loaded actor, provider identity and
its deterministic signature, registrations, owner, fingerprint and liveness.

The adapter validates the actual raw query HMAC and timestamp window before
serving private reads, rejects duplicate parameters and unknown keys, and pins
the product namespace plus any forwarded upstream authority. Public data and
key-only read-feed token acquisition do not establish proof. Spot validates the
canonical user-data subscription HMAC and correlated subscription/heartbeat
replies. Trading HTTP methods and socket operations are rejected. Both valid
synthetic pairs and tampered/unsigned/stale inputs have fixture controls.

The committed red checkpoint deliberately left the fake private venue unchecked:
three verifier controls failed, and the real Session wrong-key test failed with
`unproved socket attached`. Wiring the verifier makes the suite pass; it does
not change production admission behavior or add a proof bypass.

Run from `sdk-rs`:

```sh
cargo test --locked -j2 -p safe-cex-exchange-server --test negative_join_binance -- --test-threads=1
```

All traffic stays on loopback. No catalog, eligibility, origin, signing codec or
WASM input changes are part of this leaf.

Synthetic authentication refusals carry HTTP 401 with the negative venue code;
ordinary replies remain 200. A bounded direct loopback regression first observed
200 instead of 401 and fails before that family mapping is installed. This
corrects the fake wire protocol while retaining the permanent original-binding
refusal assertion. The earlier independent intermittent refusal-classification
failure remains recorded for the corrected-head review; no native error or
admission outcome is injected or changed here.
