/**
 * The ONE definition of how a Tealstreet script is typechecked.
 *
 * This module is deliberately dependency-free — no `typescript`, no node
 * builtins — so the browser can import it directly
 * (`@tealstreet/script-authoring/src/compilerOptions`) without dragging the
 * compiler or `node:fs` into a client bundle.
 *
 * ## Why this exists
 *
 * Three places typecheck the same source and they must not disagree:
 *
 * 1. `validateScript.ts` — the server validator.
 * 2. `packages/script-authoring/src/scripts-templates.ts` — the `tsconfig.json` seeded into
 *    `~/.tealstreet/scripts/`, which is what the user's own editor obeys.
 * 3. The in-app Monaco editor.
 *
 * A mismatch produces the worst bug class this feature can have: "the editor
 * says it's fine but the server rejects it". Monaco's
 * `typescript.typescriptDefaults` is a process-wide singleton, so it is
 * especially easy to end up laxer in the browser than on the server — the
 * existing module editor sets `strict: false`, and inheriting that for scripts
 * would silently pass code the server refuses.
 *
 * A drift test asserts (1) and (2) agree with this file.
 */

/** Values match the TypeScript enum members, given as literals so this file
 *  needs no `typescript` import. */
export const SCRIPT_COMPILER_OPTIONS = Object.freeze({
  /** ts.ScriptTarget.ESNext */
  target: 99,
  /** ts.ModuleKind.ESNext */
  module: 99,
  /**
   * ts.ModuleResolutionKind.Bundler.
   *
   * Monaco's bundled TypeScript predates this member, so its enum has no
   * `Bundler` — the numeric literal is the only portable way to express it and
   * is why this table stores numbers rather than enum references.
   */
  moduleResolution: 100,
  strict: true,
  noEmit: true,
  isolatedModules: true,
  esModuleInterop: true,
  allowSyntheticDefaultImports: true,
  skipLibCheck: true,
  types: [] as string[],
});

/** The same options as the JSON the CLI seeds into the user's scripts dir. */
export const SCRIPT_TSCONFIG_JSON = Object.freeze({
  target: 'ESNext',
  module: 'ESNext',
  moduleResolution: 'bundler',
  strict: true,
  noEmit: true,
  isolatedModules: true,
  esModuleInterop: true,
  allowSyntheticDefaultImports: true,
  skipLibCheck: true,
  types: [] as string[],
});

/**
 * Marker owner for issues produced by the Tealstreet validator.
 *
 * Kept distinct from Monaco's built-in `'ts'` owner on purpose: the browser's
 * bundled TypeScript is a different version from the server's, so the two
 * disagree on codes and wording. Sharing an owner would let one clobber the
 * other's markers on every update.
 */
export const SCRIPT_MARKER_OWNER = 'tealstreet-script';
