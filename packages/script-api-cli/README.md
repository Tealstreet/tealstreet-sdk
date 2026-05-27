# @tealstreet/script-api-cli

CLI-runtime extensions for the Tealstreet custom-scripts kernel. Reserved for things genuinely tied to the REPL runtime — hook-fire metadata, prompt state, originating-session symbol/account context.

Mounted by `apps/cli` (standalone) and `apps/web` (when running a script through the web CLI panel for capture-syntax interop).

`api.io.print` / `api.io.return` live in `@tealstreet/script-api-core`, not here — both the standalone REPL and web inline invocation consume them.

Spec source of truth: [`docs/custom-scripts/SPEC.md`](../../docs/custom-scripts/SPEC.md) (§2.2).

Status: Phase 0 scaffold. No implementations yet.
