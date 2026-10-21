# Engine rollback through cutover — docs-only plan

After #1696 merges and the browser threshold proposal is sent using at least
three complete landing reports, start a fresh branch from current landing.
Create docs/plans/RUST_ROLLBACK.md; update the Owner decision 4 link and its
stale TS-reachability wording in RUST_AUDITS.md, plus the single README index.
No source, schema, flags, defaults, deployments or live requests change.

Re-verify the SDK factory/worker/registries, options/account mapping, hosted
signing delegates, MCP config and CLI factory. Use codex-cxxyqr's pinned read-only
NX audit for persisted settings, init allowlist, hosted private/public/restart
constructors and CLI/signing-host call sites; distinguish actual mirror source
from NX's older vendored SDK and deployed binaries.

Compare three placement options: SDK resolver, NX setting/venue policy and
server flag. Recommend one shared SDK selection primitive with caller-owned
policy resolved before construction, initial hosted per-venue policy matching
its venue-keyed public feeds, explicit engine availability and effective-engine
reporting. Preserve current defaults until a later approved implementation.
Document required propagation and cache identity; no automatic live-write replay
or implicit hot switch. Keep production TS code/builds/imports/signer delegates
and executable performance oracles reachable through cutover. Coinbase/Orderly
retain hosted TS delegates; TxFlow has no production TS fallback; test references
are not production rollback coverage.

Specify approved capture/canary/read-only shadow and 3–7 day soak evidence before
TS deletion, including products/aliases/hosts, parity limits, rate/signing risks,
unknown-command reconciliation, clean dispose/reinitialize rollback and tested
fallback availability. State costs, operational tradeoffs, missing decisions
and separate implementation/deployment proof. No invented live results.

Verification: source anchors and pins, local link/index checks, unchanged
outside-scope audit sections, independent plan/full-doc review, root checks and
CI, separate exact three-doc trial/revert guard, exact-head merge and pin report.
