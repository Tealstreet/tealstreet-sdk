# Claude Guidance

Read [`AGENTS.md`](AGENTS.md) first. It is the canonical root guidance for all
coding agents in this mirror.

Package- and app-specific `CLAUDE.md` files still apply. Before editing package
or app code, read the nearest `CLAUDE.md` and treat it as the more specific
context.

The host ownership observation/teardown contract lives in
[`sdk-rs/README.md`](sdk-rs/README.md#host-ownership-observation-and-teardown).

Native hosted diagnostic owner attribution is documented in
`sdk-rs/hosts/server/DESIGN.md` under the Sentry event allowlist.

Bybit's sparse ticker freshness contract lives in
[`sdk-rs/README.md`](sdk-rs/README.md#bybit-ticker-reference-freshness).

Bitget login-only timestamp-expiry recovery samples a bounded public clock and
re-signs on the current socket at dispatch. See `sdk-rs/venues/bitget/README.md`.

Synthetic-fixture scan provenance is documented beside exact historical records
in [`.gitleaksignore`](.gitleaksignore); current fixture construction stays in its package tests.

Bybit REST mutation release budgets and their conservative batch fallback cost
are documented in [`sdk-rs/venues/bybit/README.md`](sdk-rs/venues/bybit/README.md#bybit-rest-write-pacing).
The engine owns the clocks and fences; only a validated native `10006` rejection
is known, never resendable. Contradictory and unproven responses remain Unknown.

Hyperliquid candle reads retain their original deadline while a requested public
catalogue row is pending. Known markets stay immediate; settled missing or
explicitly excluded rows refuse. See `sdk-rs/venues/hyperliquid/README.md`.

DELEGATED is venue-generic under the same hosted eligibility as COSIGNED. Use
one runtime `DELEGATED` kill switch/per-user pin; `DELEGATED_BYBIT` is only its
compatibility alias. Mobile is DELEGATED-only; DELEGATED clients have no WASM
engine or local signing, and every write is a typed v1 action over the existing
hosted WebSocket. Current contract: `docs/plans/callers/delegated-action-frames.md`;
older mode/transport discussions are explicitly superseded where they disagree.

OKX EEA config-before-catalogue startup and bounded signature refusals are
documented in [`sdk-rs/venues/okx/README.md`](sdk-rs/venues/okx/README.md#eea-private-startup).
OKX's native write-diagnostic section also documents empty-envelope cancellation
reasons and passive per-order reporting; neither grants retry or continuation authority.

Closed join-proof v1 payloads and canonical public identity facts are documented
in `docs/plans/callers/public-key-admission.md`. The pure crypto helpers do not
enable Session admission or venue readiness. The separate SDK/WASM device dispatch
retains the original signing host and owner; it does not activate any server venue.

## Shared artifact census

Shared WASM budget and construction-policy memberships are checked against
`sdk-rs/artifact-census.json`. Registry builds reject duplicate, missing or unknown
policy venues; synthetic codegen fixtures use the test-only renderer seam.

## Rust SDK CPU checks: removed by Sam 2026-11-17

The CPU gate matrix, shared certification, their planner, Bitget paired ratio gate
and nightly gate monitor were removed by Sam 2026-11-17. Replay measurements and retained evidence remain;
required coverage and combined consumers are unchanged. The measurement below
is historical, not a current job inventory.

Measured 2026-11-16 using `gh api` for `rust.yml` runs/jobs, commit check-runs,
PR merge times and branch protection. Validate response shapes, use
`--paginate --slurp` on paged endpoints, and count the latest check per name.
Keep cancelled and incomplete runs separate from failures; `updated_at` alone
does not establish completion. These are observations, not fixed job counts:

| Evidence | Observed result |
| --- | --- |
| [PR #3572 run 43924049794](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/43924049794), head `6b7aaca074c4` | 148 latest-per-name checks; 32 failed: 31 `Rust / CPU gate (<suite>)` checks and `Rust / shared CPU certification`. |
| [Master push 43942850047](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/43942850047), head `9fa98a6e3417` | All 31 venue CPU gates and shared CPU certification failed; `Rust / required coverage` succeeded. |
| Latest 12 master pushes, [43833710373](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/43833710373) through [43957235270](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/43957235270) | All 11 completed runs failed (created 08:44–19:46 UTC); the newest was still in progress. |
| Latest 30 Rust SDK runs, [43847198220](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/43847198220) through [43957235270](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/43957235270) | 19 failed with CPU-gate failures, 8 cancelled, 1 succeeded, 2 incomplete. Cancelled runs are not counted as failures. |
| [PR #3579 run 43871164658](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/43871164658), head `fbb59e4455d4` | All 31 venue CPU gates completed at 13:59:29–13:59:35 UTC, after the [13:55:34 UTC merge](https://github.com/Tealstreet/tealstreet-sdk/pull/3579). |

At measurement time, the source condition in [rust.yml](.github/workflows/rust.yml) was
`always() && needs.selection.outputs.replayRequired == 'true'`.
[ci-selection.mjs](sdk-rs/harness/ci-selection.mjs) selects `full: !isPr`, so
those jobs ran on selected PRs **and master**, not PRs alone. Their cardinality
tracked `cpu-final.strategy.matrix.suite`; the measured matrix had 31 entries.

Branch protection requires `ci` and `Rust / required coverage`, not these CPU
checks. The required-coverage job does not depend on the CPU jobs, and
`beta-wasm-release` depends on required coverage rather than CPU certification.
Historically, CPU failures could coexist with a merge/release and conclude after
a PR merged. The dated observations above explain those aggregate red runs.
The retained replay measurements still record CPU and memory results; they do
not publish the removed CPU checks. Required coverage and release dependencies
are unchanged.

Decision: removed by Sam 2026-11-17, on master and pull requests. The shared
measurement helpers remain for replay collection and offline evidence inspection;
CI no longer plans, certifies or publishes CPU gate statuses.

Update `sdk-rs/harness/CLAUDE.md` with the next real fingerprint mover; its CPU gate policy is superseded by this decision.

Hyperliquid pre-send `/info` admission diagnostics retain timeout/NotSent with the
closed `info_admission` cause. The relay/post-handoff boundary is documented in
`sdk-rs/README.md` under Browser request-failure diagnostics.

Bybit's closed later-page read diagnostics and owning-result completeness bridge
are documented in `sdk-rs/venues/bybit/README.md` under Later-page history failure
diagnostics. They add no pagination, retry or write authority.

BingX sent cancel/amend invalid-input diagnostics carry only closed request shape;
its bounds and replacement-provenance semantics live in `sdk-rs/venues/bingx/README.md`.
