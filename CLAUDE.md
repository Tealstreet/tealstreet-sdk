# Claude Guidance

Read [`AGENTS.md`](AGENTS.md) first. It is the canonical root guidance for all
coding agents in this mirror.

Package- and app-specific `CLAUDE.md` files still apply. Before editing package
or app code, read the nearest `CLAUDE.md` and treat it as the more specific
context.

The host ownership observation/teardown contract lives in
[`sdk-rs/README.md`](sdk-rs/README.md#host-ownership-observation-and-teardown).

Synthetic-fixture scan provenance is documented beside exact historical records
in [`.gitleaksignore`](.gitleaksignore); current fixture construction stays in its package tests.

DELEGATED is venue-generic under the same hosted eligibility as COSIGNED. Use
one runtime `DELEGATED` kill switch/per-user pin; `DELEGATED_BYBIT` is only its
compatibility alias. Mobile is DELEGATED-only; DELEGATED clients have no WASM
engine or local signing, and every write is a typed v1 action over the existing
hosted WebSocket. Current contract: `docs/plans/callers/delegated-action-frames.md`;
older mode/transport discussions are explicitly superseded where they disagree.
