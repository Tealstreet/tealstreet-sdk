# Blofin negative joins

`negative_join_blofin.rs` runs the real shared Session/Registry/native probe/HPKE
suite for configured mainnet and demo/testnet. All traffic stays on loopback;
these are not live venue or sandbox certification.

Each scope exercises a genuine second valid pair, wrong COSIGNED key, prior REST
signature over different full signing bytes, consumed response, both mismatched
handover credential shapes, old/rebound handover and consumed handover. The
shared assertions preserve the loaded actor/provider, deterministic signature,
owner, fingerprint, registrations and liveness with zero trading writes.

The independent fake venue validates the native Base64-of-lowercase-hex HMAC;
raw digest Base64 is a different wire. It binds the actual GET path/query, stamp,
nonce, singular key/passphrase headers and native login. The forwarded authority
is pinned to the configured network. Public discovery cannot prove possession.
WS prerequisites get a correct current answer during REST replay; raw ping/pong
is separate from JSON acknowledgments. Unknown methods/operations fail the fixture.

The committed sensitivity checkpoint leaves synthetic authentication unchecked:
three fixture controls fail, and the real mainnet Session wrong-key case fails
with `unproved socket attached`. Wiring verification makes those controls pass;
production admission and all permanent original-binding predicates are unchanged.

From `sdk-rs`:

```sh
cargo test --locked -j2 -p safe-cex-exchange-server --test negative_join_blofin -- --test-threads=1
```

Only family adapter/runner/note paths belong to the leaf. No shared helper,
readiness, eligibility, origin, signing codec or canonical WASM input changes.
