# OKX negative joins

`negative_join_okx.rs` uses the real shared Session/Registry/probe/HPKE suite for
`okx`, `okxdemo`, `okxeu` and `okxeudemo`, with synthetic loopback venue I/O.
Each exact scope exercises an independent second valid pair, wrong COSIGNED key,
previous-attempt REST signature, consumed response, both mismatched handover
credential shapes, old/rebound seal and consumed handover. The shared assertion
keeps the loaded resident actor/provider, deterministic signing result, owner,
fingerprint, registrations and liveness intact with no trading writes.

The adapter independently checks Base64 HMAC over the exact REST timestamp,
method, path and query; public responses cannot replace private proof. Config
and EEA keyed instrument discovery remain private reads. Passphrase and key
headers must be singular. Demo scopes require exactly one simulated-trading
header with value 1; mainnet scopes do not carry it. The forwarded native HTTP
authority is pinned to its configured exact venue. WS login checks its native
seconds preimage and passphrase; its signature stays fresh during REST replay
controls. Raw ping/pong remains separate from JSON acknowledgments. Unsupported
HTTP methods and socket operations fail the fixture.

The committed sensitivity checkpoint deliberately left synthetic REST/login
verification unchecked: three fixture controls fail, and the real mainnet
Session wrong-key case fails with `unproved socket attached`. Wiring real fixture
verification makes those controls pass without changing production admission.
These tests do not certify live venue access or sandbox availability.

From `sdk-rs`:

```sh
cargo test --locked -j2 -p safe-cex-exchange-server --test negative_join_okx -- --test-threads=1
```

This family leaf changes no shared helper, catalog, eligibility, signing codec,
origin or canonical WASM input.

The F1 wire regression opens the actual loopback fixture for every alias and
uses the native `WsLogin` integer timestamp shape with both legitimate pairs.
It also sends corrupt MAC/key/passphrase, null and fractional timestamp controls.
The fixture accepts native unsigned integers and canonical legacy integer strings;
it never rounds floats or relaxes signing/passphrase validation. The regression
compiled and failed before this representation fix. Broad private-read admission
success alone had not proved the WS fixture's positive authentication path.
