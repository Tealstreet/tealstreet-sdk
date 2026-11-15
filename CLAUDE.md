# Claude Guidance

Read [`AGENTS.md`](AGENTS.md) first. It is the canonical root guidance for all
coding agents in this mirror.

Package- and app-specific `CLAUDE.md` files still apply. Before editing package
or app code, read the nearest `CLAUDE.md` and treat it as the more specific
context.

The host ownership observation/teardown contract lives in
[`sdk-rs/README.md`](sdk-rs/README.md#host-ownership-observation-and-teardown).

Bybit's sparse ticker freshness contract lives in
[`sdk-rs/README.md`](sdk-rs/README.md#bybit-ticker-reference-freshness).

Synthetic-fixture scan provenance is documented beside exact historical records
in [`.gitleaksignore`](.gitleaksignore); current fixture construction stays in its package tests.

Bybit REST mutation release budgets and their conservative batch fallback cost
are documented in [`sdk-rs/venues/bybit/README.md`](sdk-rs/venues/bybit/README.md#bybit-rest-write-pacing).
The engine owns the clocks and fences; only a validated native `10006` rejection
is known, never resendable. Contradictory and unproven responses remain Unknown.

DELEGATED is venue-generic under the same hosted eligibility as COSIGNED. Use
one runtime `DELEGATED` kill switch/per-user pin; `DELEGATED_BYBIT` is only its
compatibility alias. Mobile is DELEGATED-only; DELEGATED clients have no WASM
engine or local signing, and every write is a typed v1 action over the existing
hosted WebSocket. Current contract: `docs/plans/callers/delegated-action-frames.md`;
older mode/transport discussions are explicitly superseded where they disagree.

OKX EEA config-before-catalogue startup and bounded signature refusals are
documented in [`sdk-rs/venues/okx/README.md`](sdk-rs/venues/okx/README.md#eea-private-startup).

Closed join-proof v1 payloads and canonical public identity facts are documented
in `docs/plans/callers/public-key-admission.md`. The pure crypto helpers do not
enable Session admission, device signing dispatch or venue readiness.
