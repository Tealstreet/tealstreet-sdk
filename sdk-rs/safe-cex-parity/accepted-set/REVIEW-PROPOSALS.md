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
