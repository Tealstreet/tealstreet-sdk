import type { ReadMethodName, WriteMethodName } from '@tealstreet/script-api-core/src/api/capabilities.types';

import { OEMS_READ_API_PATHS, OEMS_WRITE_API_PATHS } from '@tealstreet/script-api-core/src/runtime/runtimeSurface';

/**
 * The OEMS slice — which methods of the SHARED SDK a caller may use when it is
 * acting on the user's behalf rather than rendering a panel.
 *
 * ## This is a slice of one surface, not a surface of its own
 *
 * It used to name the browser module-sandbox's `orders.*` RPC verbs, which
 * made it a fourth order vocabulary in a repo that already had too many: the
 * sandbox protocol, `ModuleAPI['orders']`, and the SDK's own
 * `api.exchange.*`. Only the last of those crosses runtimes — it is what
 * custom scripts, hotkeys and the CLI already run on — so it is the one that
 * wins, and this file is now derived from it rather than parallel to it.
 *
 * The names below are `WriteMethodName` / `ReadMethodName` from
 * `@tealstreet/script-api-core`. Anything a caller here can do, a script can
 * do; the slice only ever subtracts.
 */

const stripPrefix = (path: string): string => path.slice(path.lastIndexOf('.') + 1);

/**
 * Derived from `OEMS_WRITE_API_PATHS` rather than retyped, so there is exactly
 * one place the slice is decided. The exclusions and their reasons live with
 * that array in the SDK.
 */
export const OEMS_WRITE_METHODS = Object.freeze(OEMS_WRITE_API_PATHS.map(stripPrefix) as readonly WriteMethodName[]);

export const OEMS_READ_METHODS = Object.freeze(OEMS_READ_API_PATHS.map(stripPrefix) as readonly ReadMethodName[]);

const WRITE_SET: ReadonlySet<string> = new Set<string>(OEMS_WRITE_METHODS);
const READ_SET: ReadonlySet<string> = new Set<string>(OEMS_READ_METHODS);

export type OemsWriteMethod = WriteMethodName;
export type OemsReadMethod = ReadMethodName;

export const isOemsWriteMethod = (value: unknown): value is OemsWriteMethod =>
  typeof value === 'string' && WRITE_SET.has(value);

export const isOemsReadMethod = (value: unknown): value is OemsReadMethod =>
  typeof value === 'string' && READ_SET.has(value);
