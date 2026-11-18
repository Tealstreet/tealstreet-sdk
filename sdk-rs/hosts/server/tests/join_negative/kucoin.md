# KuCoin Futures negative joins

This is a separate family leaf using the real shared Session/Registry/native
probe/HPKE harness. Seven mainnet scenarios include a genuinely valid second
pair, stale/consumed COSIGNED replies and mismatched/replayed/consumed sealed
credentials. Both signing modes refuse hosted testnet before private I/O or
attachment. No sandbox mapping or sibling activation is introduced.

The independent fake venue pins `api-futures.kucoin.com` and validates exact method/path/query,
request and separate passphrase Base64 HMACs, singular credential headers, key
version 2 and a fresh timestamp. The device answers the actual `kucoin-batch`
envelope with a bound credential and two MACs; no passphrase crosses that reply.
Bullet POSTs only acquire read-feed tokens, and unknown writes/operations fail.
Existing welcome-first sockets acknowledge closed public/private topic sets.
Old/new full signed request bytes must differ before rebinding a replay ID.

Strict refusals retain the loaded actor/provider, deterministic signing result,
owner, fingerprint, holdings, registrations and liveness, with zero trading
writes. No production classifier, policy, catalog, codec or shared helper changes.
Everything stays on loopback, not live venue certification.

The compiled unchecked-authentication checkpoint yields verifier 1 pass / 3
intended failures and real Session wrong-key `unproved socket attached` failure,
also reproduced after the plain master merge. Ordinary verification fixes only
the fixture. From `sdk-rs`:

```sh
cargo test --locked -j2 -p safe-cex-exchange-server --test negative_join_kucoin -- --test-threads=1
```
