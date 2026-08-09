/**
 * Independent from CORE_API_VERSION and CLI_API_VERSION — see SPEC §2.3.
 *
 * 1.0.0 — WebApi surface: `ui` token resolvers ($click / $ordersize /
 *         $quick1-7), `domHotkey` registration seam, and host-augmented
 *         `react` / `components` interfaces. `WebAdapter` is the contract
 *         `apps/web` implements; the implementation cannot live here (it
 *         needs the jotai store and controllers — see
 *         docs/custom-scripts/CONVERGENCE-AUDIT-v1.md §B3).
 *
 * Raising this from 0.0.0 is strictly more permissive: `targetCheck` only
 * rejects when the runtime provides LESS than a script requires, and a
 * script's omitted `webApiVersion` is treated as 0.0.0.
 */
export const WEB_API_VERSION = '1.0.0';
