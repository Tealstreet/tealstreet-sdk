import { describe, expect, it } from 'vitest';

import { TSCONFIG_TEMPLATE } from './scripts-templates';
import { SCRIPT_COMPILER_OPTIONS, SCRIPT_TSCONFIG_JSON } from './compilerOptions';

/**
 * The whole point of a single shared definition is that these cannot drift.
 *
 * If they do, the user's editor and the server validator disagree, and the
 * symptom is the worst one available: "the editor says it's fine but the
 * server rejects it", or worse, the inverse — code that validates and then
 * fails at runtime.
 */
describe('script compiler options', () => {
  it('matches the tsconfig the CLI seeds into the user scripts directory', () => {
    const seeded = JSON.parse(TSCONFIG_TEMPLATE) as { compilerOptions: Record<string, unknown> };
    expect(seeded.compilerOptions).toEqual(SCRIPT_TSCONFIG_JSON);
  });

  it('keeps the numeric and JSON forms describing the same configuration', () => {
    // Same keys in both representations — a flag added to one and forgotten in
    // the other is exactly how these drift.
    expect(Object.keys(SCRIPT_COMPILER_OPTIONS).sort()).toEqual(Object.keys(SCRIPT_TSCONFIG_JSON).sort());

    for (const key of Object.keys(SCRIPT_TSCONFIG_JSON)) {
      const jsonValue = (SCRIPT_TSCONFIG_JSON as Record<string, unknown>)[key];
      const enumValue = (SCRIPT_COMPILER_OPTIONS as Record<string, unknown>)[key];
      // Booleans and arrays must be literally identical; only the three enum
      // fields legitimately differ in representation.
      if (typeof jsonValue === 'boolean' || Array.isArray(jsonValue)) {
        expect(enumValue).toEqual(jsonValue);
      }
    }
  });

  // strict:false is the single most dangerous drift: the in-app Monaco editor
  // shares a process-wide `typescriptDefaults` singleton with the module
  // editor, which sets strict:false. Inheriting that would let the browser
  // pass code the server refuses.
  it('is strict', () => {
    expect(SCRIPT_COMPILER_OPTIONS.strict).toBe(true);
    expect(SCRIPT_TSCONFIG_JSON.strict).toBe(true);
  });

  // Monaco's bundled TypeScript has no `Bundler` member, so the numeric
  // literal is load-bearing rather than a shortcut.
  it('expresses moduleResolution Bundler numerically', () => {
    expect(SCRIPT_COMPILER_OPTIONS.moduleResolution).toBe(100);
    expect(SCRIPT_TSCONFIG_JSON.moduleResolution).toBe('bundler');
  });

  it('carries no ambient @types', () => {
    expect(SCRIPT_COMPILER_OPTIONS.types).toEqual([]);
  });
});
