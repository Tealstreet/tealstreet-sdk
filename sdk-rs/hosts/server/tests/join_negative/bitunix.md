# Bitunix negative joins

The shared production Session/Registry/probe/HPKE harness checks seven scenarios
for each configured mainnet/simulated-header scope, including an independently
valid second pair. Both scopes use `fapi.bitunix.com`; the latter requires one
`x-simulated-trading: 1` header, not an invented separate sandbox origin.

The fake venue independently verifies double SHA256: hex(inner digest) + secret,
with the inner digest over nonce + timestamp + key + sorted delimiter-free query.
Singular authority headers, nonce, freshness, query and native numeric WS login
are bound. The wrong single-hash construction is rejected. Full old/new proof
bytes differ before replay; no route binding is claimed beyond this protocol.

Every negative join keeps the fully loaded resident actor/provider, deterministic
signature, owner, fingerprint, holdings and original registrations live, with no
trading writes. There is no production auth/classification, policy, catalog,
codec or shared-helper change. All I/O is loopback, not live venue certification.

The compiled unchecked fixture checkpoint yields one verifier pass, three
intended verifier failures and a real Session wrong-key `unproved socket attached`
failure. The same sensitivity was reproduced after the plain master merge;
verification alone fixes the fixture. Run from `sdk-rs`:

```sh
cargo test --locked -j2 -p safe-cex-exchange-server --test negative_join_bitunix -- --test-threads=1
```
