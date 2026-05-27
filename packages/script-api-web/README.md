# @tealstreet/script-api-web

Web-runtime extensions for the Tealstreet custom-scripts kernel. Exposes React + the lifted `api.components.*` surface, UI-token resolvers (`$click`, `$ordersize`, `$quick1-7`), and DOM-source hotkey trigger registration.

Mounted by `apps/web` only. Inaccessible from standalone CLI — a script declaring `targets: ["core", "web"]` is hard-rejected at load on the standalone runtime per SPEC §2.5.

Note: this is **not** a superset of today's `ModuleAPI`. Per SPEC §11.5, legacy custom React modules + hotkeys keep running on their existing `executeHotkeyCode` path indefinitely (dual-path-forever). `script-api-web` is the lean, async-first, opt-in surface for new-shape scripts.

Spec source of truth: [`docs/custom-scripts/SPEC.md`](../../docs/custom-scripts/SPEC.md) (§2.3, §11.5).

Status: Phase 0 scaffold. No implementations yet.
