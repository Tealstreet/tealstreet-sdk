# Offline bundle rollout preparation

This directory changes no product flag, session, application, server or deployment.
See `docs/plans/RUST_BUNDLE_ROLLOUT.md` for Sam's per-user bundle decision.
Commands have no apply mode. Exit 0 never authorizes a live cohort.

```sh
corepack yarn node scripts/rust-rollout/cli.mjs matrix
corepack yarn node scripts/rust-rollout/cli.mjs evaluate scripts/rust-rollout/example.json
corepack yarn node --test scripts/rust-rollout/model.test.mjs
```

The example is entirely synthetic, including its digests and proof strings. It
returns **exit 2 / blocked** with the committed source matrix, even though the
synthetic declarations are filled: no live account shapes or frozen rollback
have been certified. Tests use a separate fictitious policy to exercise the
positive declaration path; they do not modify the committed matrix.

## Rehearsal input

`example.json` is the complete shape. Unknown fields and credential fields are
rejected. Never supply real keys, account state, signed URLs or session tokens.
Use pseudonyms and synthetic account IDs only; error output uses account
ordinals, not input values.

- `assignment`: one user target (`rust`), revision and pseudonym; no account selector.
- `artifacts.frozen` / `.rust`: SHA-256 digests for app, server, worker and WASM,
  wire version, effective engine and supported alias list. Frozen TS WASM digest
  identifies the retained manifest's WASM artifact if present; if there is none,
  use the manifest digest identifying its declared empty WASM set. Do not hash a
  nonexistent file or pretend the TS app runs a Rust engine.
- `accounts`: the complete held-account inventory, each with a unique synthetic
  ID, exact venue alias and reviewed shape. Completeness requires external review;
  the tool cannot discover an omitted real account.
- `sessions`: exactly one per account, effective client and server engine,
  matching revision/artifact/wire identity, former-writer retirement, revoked
  resume rights, zero unreconciled writes and transferred protection ownership.
- `storage`: reference to an actual old->new->old fixture result, affirmative
  backward compatibility/frozen DB open, and `cacheAuthority: "display-only"`.
- `controls`: evidence references for internal isolation, kill switch,
  predeclared monitoring thresholds, rollback rehearsal and Sam approval. Opt-in and
  widening also require `optInConsent`; widening requires `priorCohortReview`.
- `evidence`: one per venue/shape, bound to the SDK commit and Rust app/server
  digests; consumer, public quiet/volatile, private shadow, smoke, session parity,
  rollback and supported-product scope references, seven observed days and a
  maintenance/reconnect event. Smoke means every applicable step passed; an
  unsupported step needs an explicit reviewed product scope, not fabricated PASS.

References are opaque review pointers. The tool checks presence and identity,
not whether external evidence exists, is trustworthy, or meets its own criteria.
`declarationsComplete: true` therefore still returns `authority: "none"` and
`liveEvidenceVerified: false`. Empty evidence, unknown shape, legacy disposition
or a matrix blocker prevents declaration completeness. Updating the pinned
matrix to admit a shape requires a reviewed policy change and actual evidence.

## Artifact measurement

```sh
corepack yarn node scripts/rust-rollout/cli.mjs compare-artifacts /absolute/path/frozen-assets.json /absolute/path/rust-assets.json
```

Each manifest has this shape (illustrative numbers only):

```json
{
  "schema": 1,
  "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "files": [
    {
      "path": "app.js",
      "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "bytes": 100,
      "gzipBytes": 60,
      "brotliBytes": 50
    }
  ]
}
```

Capture complete manifests externally using the same build/compression settings.
The tool performs no builds, file mutations or network requests. It compares
added/modified/removed paths, additive-only status, totals and deltas. It refuses
missing sizes, duplicate paths and inconsistent sizes for identical bytes.
Measure source-diff scope and initial/lazy/server asset graphs separately.
Manifest completeness and actual frozen/candidate identity need independent
review; arbitrary JSON is not authenticated build provenance.
