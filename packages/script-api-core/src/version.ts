/**
 * Independent of the npm `version` in package.json. Bumped only when the
 * runtime-visible CoreApi contract changes (additions allowed in minor,
 * breaking changes require major). Phase 1 ships the full read-side
 * surface + ApiProxy seam + targetCheck.
 *
 * 1.0.0 — Phase 1: read-side getters, ApiProxy + InProcessProxy, AdapterContext,
 *         targetCheck, accountResolver, utils, constants. Action/io/storage
 *         types ship but impls land in later phases.
 * 1.1.0 — Add typed Bybit BBO and TickSize execution controls to place-order
 *         inputs. Hosts pass these POJO fields through unchanged.
 * 1.1.1 — No contract change. `script.ts` gained a corrected doc-path
 *         reference when the types-bundle test moved to
 *         `packages/script-authoring`; bumped because `check:api-versions`
 *         watches the file, and a patch bump is cheaper than teaching the
 *         gate to distinguish comments from surface.
 */
export const CORE_API_VERSION = '1.1.1';
