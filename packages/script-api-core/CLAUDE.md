# CLAUDE.md — @tealstreet/script-api-core

Spec source of truth: [`docs/custom-scripts/SPEC.md`](../../docs/custom-scripts/SPEC.md). Update there, not here, until this package has gotchas worth documenting.

## Review notes

- **`api.exchange.runCommand` depth is a recursion guard, not a parallelism
  limiter.** The spec describes capping recursive `runCommand` dispatch
  (for example script -> runCommand -> script -> runCommand). Current
  implementation carries depth per dispatch context via
  `RunContext.runCommandDepth`; concurrent sibling calls from the same
  script all dispatch at depth 1. The internal proxy envelope carries
  `depth` to the host adapter, but the public script-facing
  `api.exchange.runCommand(command, opts)` options remain only
  `{ account?, symbol? }`.
