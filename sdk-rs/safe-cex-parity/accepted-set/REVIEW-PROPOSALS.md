# Review missing contracts

From the repository root, generate a review document for one census crate:

```sh
node sdk-rs/safe-cex-parity/scripts/review-contract-proposals.mjs \
  --crate=safe-cex-venue-woo \
  --census=/Users/sam/cs/tealstreet-sdk/.aimux/plans/parity-accepted-contract-census.json \
  --out=/tmp/woo-review-proposals.json
```

Use an unused output filename outside the checkout, or omit `--out` for JSON on
stdout. The helper runs fresh native/default rustdoc for the selected package's
production targets. It reads the census, compiler declarations, source bodies
and current classifications. It never writes contract or source files.

Read the selected crate's nearest `CLAUDE.md` before running Cargo. The helper
defaults to two Cargo jobs; add `--jobs=1` for crates such as Bitunix and Blofin
that require one. This sets both the Cargo environment and rustdoc job flag.
Targets run sequentially. Follow any crate restriction on concurrent Cargo
processes when scheduling other commands alongside the helper.

The accepted-set runner supports the same bounded flag, for example
`node sdk-rs/safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-bitunix --jobs=1`.
It applies to compiler discovery, the host scanner build and native/wasm probe
compilation. Callers of `runProbes` can set `configuration.jobs` to `1`;
omitting it retains two jobs. This controls compiler concurrency only and does
not change inventory, classification requirements or runtime assertions.

Each proposal includes the exact compiler owner/API, source spans and file
hashes, bounded source excerpts, candidate classifications and required native
witnesses. Text references in maintained test files are suggestions to inspect;
they are explicitly **unexecuted**, and compiler discovery does not establish
native behavior. Existing classifications are included when the census predates
them, so reviewers can avoid duplicating another lane's work.

The report has no executable contract `category`. Review every body and the
corresponding TS/tooling contract before authoring a classification. Author
expected outputs independently and execute the actual native APIs. A source
hash, spelling heuristic, test reference or successful compilation does not
replace a runtime decoder witness. Non-decoder reasons must describe each
specific function's role. The report cannot be passed to `--contracts`.

Receiver refusals stay visible. A census target blocked by a reviewed-source
failure produces a blocked report, never a zero or an exemption. Missing/stale
compiler declarations fail explicitly. Alias proposals expose raw compiler
targets; expanded aliases, root identity and actual conversion traits still
need their normal checks. Selected census keys do not establish complete
discovery, local-scope coverage or the feature/wasm matrix.

For positional-array widening of TS object records or tagged-map widening of
string-literal enums, use the shared
[ingress recipes and proof requirements](MAP-ONLY-INGRESS.md).

Use the maintained boundary decisions when reviewing
[strings](../../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[typed outputs](../../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[snapshots](../../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[numeric ingress](../../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[divergence](../../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[surrogate observability](../../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
After manually editing reviewed contracts, run their scoped native probes and
the accepted-set gates required for the change. Proposal generation is not a
passing gate or permission to accept a missing contract.


Reviewed classifications live in `contracts/<compiler-crate>.json`, using the
compiler namespace with underscores replaced by hyphens. For example,
`safe_cex_venue_okx::types::RegionWire` belongs in
`contracts/safe-cex-venue-okx.json`. Edit only your crate file; retain the existing
flat owner-to-contract JSON shape. The former shared `contracts.json` is removed.
The gate and this helper load the complete directory through
`../scripts/accepted-contract-files.mjs`; native proof and all validation rules
are unchanged. Custom fixture files still work with `--contracts=<file.json>`.

Contract loading retains authored JSON tokens for native input. In particular,
`{"9":...,"2":...,"x":...}` reaches Rust in that authored order, so the Rust
implementation must perform any TS numeric-key ordering itself. Number spelling
(including `-0` and values outside JS's exact integer range), escaped keys and
nested cases are preserved through generated multi-entry dispatch. Use
`loadContracts()` and the shared `runProbes()` path; a separate `JSON.parse` /
`JSON.stringify` round trip or `structuredClone` of cases loses this source
metadata. Programmatically constructed fixtures have only their JS value and
cannot claim an authored ordering or numeric spelling they no longer retain.
When appending to a loaded shard, write it with `stringifyJsonWithSource` from
`../scripts/accepted-json-source.mjs` or edit the authored text directly. Rewriting
existing cases with ordinary `JSON.stringify` permanently discards those tokens.
