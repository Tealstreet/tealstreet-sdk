# Bitget negative joins

`negative_join_bitget.rs` uses the real shared Session/Registry/native probe/HPKE
suite for `bitget`, `bitgetusdc` and `bitgetinverse`, each on its configured
mainnet/testnet product scope. All I/O stays on loopback; this does not certify
live venue or sandbox availability.

Each scope exercises a genuine second valid pair, wrong COSIGNED key, prior REST
signature over different signing bytes, consumed reply, both mismatched handover
credential shapes, old/rebound seal and consumed handover. The loaded resident
actor/provider, deterministic signature, registrations, owner, fingerprint and
liveness remain intact; trading methods and socket operations fail the fixture.

The adapter independently verifies Base64 REST HMAC over the exact millisecond
stamp, GET path and query, singular key/passphrase headers and timestamp bound.
The v3 settings probe is private even when its valid response selects Classic;
its `40084` fallback cannot substitute for authentication. Every other request
pins its product type. Known invalid credentials return native `40006` with
HTTP 400; private WS login checks its native seconds HMAC and passphrase. WS
prerequisites get a correct current answer during REST replay. Public discovery
cannot prove possession. Simulated currency names are fixture data only.

The committed sensitivity checkpoint leaves synthetic authentication unchecked:
three verifier controls fail, and the real USDT Session wrong-key scenario fails
with `unproved socket attached`. Wiring the fake venue's verifier makes those
controls pass; no production admission or shared refusal predicate changes.

From `sdk-rs`:

```sh
cargo test --locked -j2 -p safe-cex-exchange-server --test negative_join_bitget -- --test-threads=1
```

Only this family's adapter, runner and note belong to the leaf. No shared helper,
catalog, origin, signing codec or canonical WASM input is changed.
