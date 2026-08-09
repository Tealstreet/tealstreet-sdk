import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import path from 'node:path';

import ts from 'typescript';

import { SCRIPT_COMPILER_OPTIONS } from '../compilerOptions';

/** Premys-shaped so the MCP tool, the AI repair loop, and editor markers all
 *  consume one thing. `line`/`column` are 1-indexed and omitted for
 *  diagnostics with no source position (bad lib, config errors). */
export interface ValidationIssue {
  message: string;
  line?: number;
  column?: number;
  code?: number;
}

export interface ValidationResult {
  valid: boolean;
  errors: ValidationIssue[];
  /**
   * How many errors were syntax errors. When non-zero the AST is unreliable,
   * so the composed `validate()` suppresses its other passes rather than
   * piling speculative findings on top of a parse failure.
   */
  syntacticErrorCount?: number;
}

const SCRIPT_FILE = '/script.ts';
const TYPES_FILE = '/types.d.ts';

/**
 * Derived from the single shared definition in `../compilerOptions`, which the
 * seeded user tsconfig and the in-app Monaco editor also read. Do not inline a
 * copy here — see that file for why a mismatch is the worst bug this feature
 * can have.
 */
const COMPILER_OPTIONS: ts.CompilerOptions = { ...SCRIPT_COMPILER_OPTIONS } as ts.CompilerOptions;

// `ts.getDefaultLibFilePath` resolves through `ts.sys.getExecutingFilePath()`,
// which breaks inside a `bun build --compile` binary. Resolving the package
// directly survives that; if this ever ships inside a compiled CLI binary the
// lib files need embedding as string constants instead.
const require_ = createRequire(import.meta.url);

/**
 * Resolved on FIRST USE, never at module load.
 *
 * Turbopack rewrites `createRequire(...).resolve()` to its own shim, which
 * returns a numeric module id rather than a path — so `path.dirname()` threw
 * `ERR_INVALID_ARG_TYPE` while merely EVALUATING this module. Any Next route
 * that imports this package transitively then failed to build at all, with
 * "Failed to collect page data" and a stack pointing only at a bundler chunk.
 *
 * Deferring it means a route that imports the package but never validates —
 * `/api/modules/screen` does exactly that — no longer pays for the compiler,
 * and a bundler that cannot resolve TypeScript fails where it is diagnosable
 * instead of at import time.
 */
let tsLibDir: string | undefined;

const getTsLibDir = (): string => {
  if (tsLibDir !== undefined) return tsLibDir;
  const resolved = require_.resolve('typescript');
  if (typeof resolved !== 'string') {
    throw new Error(
      `cannot locate the TypeScript lib directory: require.resolve returned ${typeof resolved}. ` +
        'This build has replaced require.resolve with a bundler shim; validation needs the real files on disk.',
    );
  }
  tsLibDir = path.dirname(resolved);
  return tsLibDir;
};

/**
 * Lib files never change, so `fileName` is a safe key for them. The types
 * bundle is NOT safe to key that way: it arrives per call, and two callers on
 * different versions (an MCP server and a web route, say) would silently share
 * whichever bundle happened to be compiled first — producing spurious errors
 * against types the caller never supplied. Its cache key therefore includes a
 * content hash.
 */
const libFileCache = new Map<string, ts.SourceFile>();
const typesFileCache = new Map<string, ts.SourceFile>();

const hash = (text: string): string => createHash('sha1').update(text).digest('hex');

const getLibSourceFile = (fileName: string, target: ts.ScriptTarget): ts.SourceFile | undefined => {
  const cached = libFileCache.get(fileName);
  if (cached) return cached;
  const text = ts.sys.readFile(fileName);
  if (text === undefined) return undefined;
  const file = ts.createSourceFile(fileName, text, target, true);
  libFileCache.set(fileName, file);
  return file;
};

const getTypesSourceFile = (typesBundle: string, target: ts.ScriptTarget): ts.SourceFile => {
  const key = hash(typesBundle);
  const cached = typesFileCache.get(key);
  if (cached) return cached;
  const file = ts.createSourceFile(TYPES_FILE, typesBundle, target, true);
  // One bundle in practice; bound it anyway so a caller varying the bundle
  // per request cannot grow this without limit.
  if (typesFileCache.size > 4) typesFileCache.clear();
  typesFileCache.set(key, file);
  return file;
};

const toIssue = (diagnostic: ts.Diagnostic): ValidationIssue => {
  const message = ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ');
  // Fileless diagnostics are real (a bad `lib` yields TS2318 with no file).
  // Never assert `diagnostic.file!`.
  if (diagnostic.file === undefined || diagnostic.start === undefined) {
    return { message, code: diagnostic.code };
  }
  const { line, character } = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start);
  // getLineAndCharacterOfPosition is 0-indexed on both axes; editors are 1-indexed.
  return { message, line: line + 1, column: character + 1, code: diagnostic.code };
};

export interface ValidateScriptOptions {
  /** The composed types bundle — CORE + CLI + WEB `.d.ts` constants. */
  typesBundle: string;
}

/**
 * Typecheck a user script against the generated API surface.
 *
 * Three properties of the setup are load-bearing and easy to break:
 *
 * 1. **`/types.d.ts` must be a rootName**, not merely present in the host.
 *    With only `/script.ts` as a root, every script fails with TS2307
 *    "Cannot find module '@tealstreet/script-api-core'".
 * 2. **The composed bundle must stay a non-module script.** Its
 *    `declare module` blocks are only *ambient* while the file has no
 *    top-level `import`/`export`. If the generator ever emits one, the blocks
 *    silently become module *augmentations* and resolution collapses —
 *    `rejectsLocalImports` in the tests pins this.
 * 3. **The host must not expose `node_modules`.** If it does, resolution finds
 *    the real `@tealstreet/script-api-core/src/index.ts` and drags safe-cex
 *    into the program, which both slows it down and validates against the
 *    wrong (host-internal) surface.
 */
export const validateScript = (source: string, { typesBundle }: ValidateScriptOptions): ValidationResult => {
  const target = COMPILER_OPTIONS.target ?? ts.ScriptTarget.ESNext;
  const scriptFile = ts.createSourceFile(SCRIPT_FILE, source, target, true);
  const typesFile = getTypesSourceFile(typesBundle, target);

  const host: ts.CompilerHost = {
    getSourceFile: (fileName) => {
      if (fileName === SCRIPT_FILE) return scriptFile;
      if (fileName === TYPES_FILE) return typesFile;
      // Gate the disk fallback on the lib dir. `fileExists` constrains module
      // resolution but NOT `/// <reference path="/abs/any.ts" />`, which asks
      // the host for an arbitrary path directly — without this guard, untrusted
      // source could read any file on the box and poison the cache with it.
      if (!fileName.startsWith(getTsLibDir())) return undefined;
      return getLibSourceFile(fileName, target);
    },
    getDefaultLibFileName: (options) => path.join(getTsLibDir(), ts.getDefaultLibFileName(options)),
    writeFile: () => undefined,
    getCurrentDirectory: () => '/',
    getCanonicalFileName: (fileName) => fileName,
    useCaseSensitiveFileNames: () => true,
    getNewLine: () => '\n',
    // Deliberately narrow: only the virtual files and the TS lib directory are
    // visible. Anything else must read as absent so module resolution cannot
    // escape into real node_modules.
    fileExists: (fileName) =>
      fileName === SCRIPT_FILE ||
      fileName === TYPES_FILE ||
      (fileName.startsWith(getTsLibDir()) && ts.sys.fileExists(fileName)),
    readFile: (fileName) => {
      if (fileName === SCRIPT_FILE) return source;
      if (fileName === TYPES_FILE) return typesBundle;
      return fileName.startsWith(getTsLibDir()) ? ts.sys.readFile(fileName) : undefined;
    },
    directoryExists: (dirName) => dirName === '/' || dirName.startsWith(getTsLibDir()),
    getDirectories: () => [],
  };

  const program = ts.createProgram({
    // Order matters only for determinism of diagnostics, but types.d.ts MUST
    // be here — see the doc comment above.
    rootNames: [TYPES_FILE, SCRIPT_FILE],
    options: COMPILER_OPTIONS,
    host,
  });

  const syntactic = program.getSyntacticDiagnostics(scriptFile);
  const diagnostics = [
    ...syntactic,
    ...program.getSemanticDiagnostics(scriptFile),
    ...program.getGlobalDiagnostics(),
    // Without these a truncated or malformed bundle validates every script as
    // clean — the worst possible failure, since it reads as "all good".
    ...program.getSyntacticDiagnostics(typesFile),
    ...program.getOptionsDiagnostics(),
  ];

  const errors = diagnostics.map(toIssue);
  return { valid: errors.length === 0, errors, syntacticErrorCount: syntactic.length };
};

/** Test seam only. Production correctness must NOT depend on calling this —
 *  the types cache is content-keyed precisely so it never needs clearing. */
export const __clearValidatorCache = (): void => {
  libFileCache.clear();
  typesFileCache.clear();
};
