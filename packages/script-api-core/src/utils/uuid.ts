/**
 * Generate a v4 UUID. Backed by `crypto.randomUUID()` which is available in
 * Node 19+, Bun, Deno, and all modern browsers — the only runtimes script-api
 * targets. See SPEC §2.1 api.utils.uuid.
 */
export const uuid = (): string => globalThis.crypto.randomUUID();
