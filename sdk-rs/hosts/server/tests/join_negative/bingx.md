# BingX perpetual negative joins

The real shared Session/Registry/probe/HPKE suite runs seven mainnet scenarios,
including a genuinely valid second pair and strict permanent refusal preserving
the resident actor/provider, deterministic signature, owner, holdings, fingerprint,
original registrations and liveness. No trading writes are released.

The independent fake venue checks singular API-key headers, exact encoded query
HMAC, a single fresh timestamp and signature. API-key-only listen-token POST is
read-only bootstrap, never possession proof; native signed private reads must
succeed. Public discovery is unsigned. Raw Ping/Pong is handled separately from
JSON public subscription acknowledgments. Unknown requests fail the fixture.

Both signing modes refuse hosted testnet before signing/private I/O or attachment.
Excluded `bingxspot` stays refused in a separate COSIGNED control. No sibling,
network, readiness or DELEGATED eligibility activation is introduced. All traffic
is loopback; configured behavior does not certify live venue availability.

The committed unchecked-authentication checkpoint makes three verifier controls
and a real Session wrong-key join fail (`unproved socket attached`). Wiring the
independent verifier changes no production classifier or shared refusal predicate.
From `sdk-rs`:

```sh
cargo test --locked -j2 -p safe-cex-exchange-server --test negative_join_bingx -- --test-threads=1
```
