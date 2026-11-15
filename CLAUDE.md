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

Bybit REST mutation release budgets and their conservative batch fallback cost
are documented in [`sdk-rs/venues/bybit/README.md`](sdk-rs/venues/bybit/README.md#bybit-rest-write-pacing).
The engine owns the clocks and fences; native rate-limit uncertainty is unchanged.
