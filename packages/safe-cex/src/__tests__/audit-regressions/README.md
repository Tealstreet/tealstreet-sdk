# Audit regressions

This directory is the trusted home for deterministic regression tests created
while resolving exchange audit findings.

- Derive expected behavior from current official upstream specifications,
  verified audit evidence, or an explicit runtime contract in the nearest
  `CLAUDE.md`.
- Do not copy or migrate assertions, fixtures, mocks, or helper code from the
  deleted legacy test corpus under `src/exchanges/**`.
- Do not use the network, API keys, `.apikey.json`, timing-sensitive live data,
  or writable exchange operations.
- Keep the regression focused on the observable contract fixed by the same
  commit. Mock only the transport boundary needed to make it deterministic.
