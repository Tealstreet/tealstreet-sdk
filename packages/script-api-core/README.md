# @tealstreet/script-api-core

Portable headless kernel for the Tealstreet custom-scripts runtime. Provides the runtime-agnostic `CoreApi` surface (snapshot getters, exchange actions, io, storage, utils, constants), the `ApiProxy` seam that lets v2 swap in worker isolation without touching script code, and the load-time target/version checks.

Mounted on every runtime — standalone CLI, web, and (future) mobile.

Spec source of truth: [`docs/custom-scripts/SPEC.md`](../../docs/custom-scripts/SPEC.md).

Status: Phase 0 scaffold. No implementations yet — see `src/` stubs for the file layout.
