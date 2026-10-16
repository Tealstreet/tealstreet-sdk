# Coinbase lane brief

The maintained TypeScript reference is `packages/safe-cex/src/exchanges/coinbase`.
Both runtimes remain maintained. The crate contains implementations, not a scaffold.

At audit parent `7216d14d553d5415600a8d45176411b4f3981a83`, independently
parsed with Python `tomllib`: **393 ported / 33 divergent / 0 pending** across
426 rows in 16 manifests. That parent merges integration
`0286f63437e4320e8c12886d5fef8fc6dfa2eac3`. The final annotation audit changes
only notes, preserving those status totals. Enumeration stops at
`coinbasespot.exchange.ts.toml`.
The previous brief's claim of 426 pending rows was stale.

All 33 divergent rows have been audited: **11 supported explanations / 8
behaviour findings / 14 rationale or proof findings**. Twenty-two notes now
begin with `FINDING`: the original sixteen plus six withdrawn explanations.
The finding split is **36.36% behaviour / 63.64% rationale or proof**; see
[the complete row ledger](DIVERGENCE_AUDIT.md). These are row counts, not
independent defects or a full-function proof. Zero statuses or targets moved;
no finding was closed by this annotation audit.

Remaining work is the eight behaviour rows and fourteen rationale/proof rows named
in that ledger. The snapshot notification bridge is already repaired; do not
reopen it solely because an older evidence paragraph describes its withdrawal.
`DefaultStore` is constructible. Shared carrier capacity does not establish
every host binding or error-property policy.

Commit locally on the lane branch. Merge integration before delivery. **Do not
push or merge into integration.** Report the branch, SHA and measured gate exits
so the coordinator can fetch locally. The current required gates, from `sdk-rs`,
are only:

```sh
cargo fmt --all --check
cargo build --workspace --locked --jobs 2
```

Run one Cargo command at a time. Host clippy, wasm clippy, wiring, layers and
routing are centrally owned; do not start duplicate runs. Use sub-agent reviews.
Do not reread `CLAUDE.md` or agent-memory for this assignment.

Canonical declarations remain in MANIFEST-SPEC
[14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
