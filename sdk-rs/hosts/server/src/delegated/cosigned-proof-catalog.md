# COSIGNED proof records

Each exact Hosted declaration has one file in `cosigned-proof-catalog/`, named
`<venue>.json`. No family aliases: variants own separate records. The server's
build script validates the complete declaration roster and generates the native
readiness set and shared test cohorts in Cargo's output directory. Generated
output is never committed. The JSON records are build inputs, not runtime files.

An unproved venue retains its exact reason:

```json
{"status":"excluded","reason":"The existing reason, unchanged."}
```

A proved venue replaces only its own record:

```json
{
  "status":"ready",
  "probeTests":"src/delegated/probe_tests.rs",
  "sessionTests":"src/session/tests/cosigned_venues.rs"
}
```

Name the actual compiled test sources. The paths above select the existing
shared HMAC probe and Session cohorts. A separate native or local possession
suite names its own source, for example `src/delegated/cryptocom_probe_tests.rs`
and `src/session/tests/cosigned_cryptocom.rs`; those entries do not enter the
shared HMAC fixtures. Bybit keeps its existing probe in `src/delegated.rs`.
These references track coverage; they never substitute for running the real
positive, wrong-key, replay, foreign-socket and ownership controls.

Files are closed JSON objects: no comments, duplicate fields, unknown keys,
missing sources or simultaneous readiness/exclusion. Proof paths must stay in
this crate's `src/` and identify existing Rust tests. Every declared venue must
be exactly ready or excluded, and exclusions must have nonempty reasons.

For an older proof PR: refresh after this structural PR lands, retain its real
proof implementation/tests, drop edits to the old exclusions array and the
three shared runtime/fixture lists, and promote only its exact JSON record(s).
Use the actual shared or standalone test paths. Preserve every other record.
Do not enable another venue or remove another exclusion. From the repository
root, run (the Rust subshell selects the pinned toolchain):

```sh
node --test sdk-rs/harness/cosigned-catalog.test.mjs
(cd sdk-rs && cargo test --locked -p safe-cex-exchange-server --test cosigned_catalog -j2)
```

The accounting guard also removes a stale origin filter: NX #9737 removed that
gate after SDK #3567/#3570 supplied HyroTrader/Binance Options origins. Both
remain metadata-disabled and unproved. The census is 41 declared, 40 helper
eligible and 33 production offered. The seven disabled helper entries stay
visible. This guard correction does not admit either venue.

The records and generator live entirely in the server crate, which the canonical
beta-WASM input walk excludes. The guard is an exempt harness test. This layout
change requires no WASM release or pin; eventual server changes use the normal
maintainer-controlled server roll.
