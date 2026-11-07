# Binance signing residue

Owner: claude-kuhwxi (overseer assignment, 2026-10-26). Reviewer: claude-4yjlw0 or claude-dazzy3. Speed mode applies: this is a venue-only PR whose inventory changes are removal-only.

## Scope

Binance's production signing is already on dispatch (`sign::Http`, `SpotSubscribe` and the hosted-signing work). All 27 legacy signing sites at base were test machinery:

| File | Base | After | What |
| --- | ---: | ---: | --- |
| `sign.rs` | 17 | 4 | a `#[cfg(test)]` legacy HMAC oracle (`LegacyCredentials`, `Signed`) and the hosted-signing recipe tests |
| `secondary_tests.rs` | 9 | 0 | raw fixture credentials, and an HMAC recomputed with them |
| `trading.rs` | 1 | 0 | a raw fixture credential |

The four that stay are the dispatch recipe's own `hmac_sha256_work` import and calls, frozen as BloFin's dispatch lines are.

## Changes

- **Oracle deleted, not moved.** Its proofs are re-made through the installed provider, in `tests/signing_vectors.rs`:
  - The documented vector (developers.binance.com, SIGNED Endpoint Examples) is checked against the primitive, and the dispatch path signs its exact query with that key.
  - Every configured receive window (absent → 20000, 500, 0, 500.125, 60000) is signed as given.
- **`secondary_tests` keep their role assertions without an oracle.** The key presented (`X-MBX-APIKEY`), the egress intent and the exact signed payload are asserted. The HMAC bytes are proved once, by the vectors above and by `tests/identity.rs`'s independent check.
- **Fixture credentials live in `tests/support/config.rs`** (`keyed`, `with_read_identity`, `without_read_signing`), the crate's existing trusted test-setup file. No venue `src` file holds a fixture key or seed.
- **The hosted-signing recipe's byte tests move unchanged** to `tests/unit/hosted_signing.rs` (`#[path]`, crate access).

No production code changes.
