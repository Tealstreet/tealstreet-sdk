import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { beforeEach, describe, expect, it } from 'vitest';

// The REAL generated bundle, not a fixture: validating against a
// hand-written surface would prove nothing about the shipped contract.
import { CORE_TYPES_DTS, WEB_TYPES_DTS } from '../script-types-bundle';
import { checkRuntimeImports } from './checkRuntimeImports';
import { validate } from './validate';
import { __clearValidatorCache, validateScript } from './validateScript';

const typesBundle = [CORE_TYPES_DTS, WEB_TYPES_DTS].join('\n');
const options = { typesBundle };

// Mirrors HELLO_TEMPLATE's shape: api/args are PARAMETERS, not ambient globals.
const wrap = (body: string) => `
import type { CoreApi, ScriptArgs } from '@tealstreet/script-api-core';

export default async function (args: ScriptArgs, api: CoreApi) {
${body}
}
`;

beforeEach(() => {
  __clearValidatorCache();
});

// Deliberately NOT inside beforeEach's cleared state: production must be
// correct without anyone calling __clearValidatorCache.
describe('types cache is content-keyed', () => {
  it("does not leak one caller's bundle into another's validation", () => {
    const a = `declare module '@tealstreet/script-api-core' { export interface OnlyInA { a: number } }`;
    const b = `declare module '@tealstreet/script-api-core' { export interface OnlyInB { b: number } }`;
    const useA = `import type { OnlyInA } from '@tealstreet/script-api-core';\nexport default async function () { const x: OnlyInA = { a: 1 }; void x; }`;
    const useB = `import type { OnlyInB } from '@tealstreet/script-api-core';\nexport default async function () { const x: OnlyInB = { b: 1 }; void x; }`;
    expect(validateScript(useA, { typesBundle: a }).valid).toBe(true);
    // Same virtual filename, different bundle: a fileName-keyed cache would
    // hand back bundle A here and fail.
    expect(validateScript(useB, { typesBundle: b }).valid).toBe(true);
    expect(validateScript(useA, { typesBundle: a }).valid).toBe(true);
  });

  it('surfaces a malformed bundle instead of passing everything', () => {
    const truncated = `declare module '@tealstreet/script-api-core' { export interface X {`;
    const result = validateScript(`export default async function () {}`, { typesBundle: truncated });
    expect(result.valid).toBe(false);
  });
});

describe('validateScript', () => {
  it('accepts a script written against the real API surface', () => {
    const result = validateScript(
      wrap('  const b = await api.getBalance();\n  api.io.return(b?.total ?? 0);'),
      options,
    );
    expect(result.errors).toEqual([]);
    expect(result.valid).toBe(true);
  });

  // Load-bearing property 1+2: /types.d.ts must be a rootName AND the bundle
  // must stay a non-module script. If the generator ever emits a top-level
  // import/export, the `declare module` blocks become augmentations rather
  // than ambient declarations and every script fails with TS2307.
  it('resolves the ambient module declaration (no TS2307)', () => {
    const result = validateScript(wrap('  void api;'), options);
    expect(result.errors.filter((e) => e.code === 2307)).toEqual([]);
  });

  it('reports a type error with a 1-indexed line and column', () => {
    const result = validateScript(wrap('  const n: number = "not a number";'), options);
    expect(result.valid).toBe(false);
    const issue = result.errors.find((e) => e.code === 2322);
    expect(issue).toBeDefined();
    // Body starts on line 5 of `wrap`, and columns are 1-indexed.
    expect(issue?.line).toBe(5);
    expect(issue?.column).toBeGreaterThan(0);
  });

  it('rejects a method that is not on the API surface', () => {
    const result = validateScript(wrap('  await api.definitelyNotAMethod();'), options);
    expect(result.valid).toBe(false);
  });

  // Load-bearing property 3: the host must not expose node_modules, or module
  // resolution escapes to the real package and validates the wrong surface.
  it('rejects a local import — the loader is one-file-per-script', () => {
    const result = validateScript(`import type { X } from './helper';\n${wrap('  void api;')}`, options);
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.code === 2307)).toBe(true);
  });

  it('does not crash on syntactically broken source', () => {
    const result = validateScript('export default async function ( {{{', options);
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);
  });
});

describe('checkRuntimeImports', () => {
  it('allows import type', () => {
    expect(checkRuntimeImports(`import type { CoreApi } from '@tealstreet/script-api-core';`)).toEqual([]);
  });

  it('allows per-specifier inline type imports', () => {
    expect(checkRuntimeImports(`import { type CoreApi } from '@tealstreet/script-api-core';`)).toEqual([]);
  });

  // The whole point: this typechecks clean but dies at load time, and it is
  // the most likely thing an AI generation loop emits.
  it('rejects a value import that the typechecker accepts', () => {
    const source = `import { OrderSide } from '@tealstreet/script-api-core';`;
    expect(validateScript(source, options).errors.filter((e) => e.code === 2307)).toEqual([]);
    const issues = checkRuntimeImports(source);
    expect(issues).toHaveLength(1);
    expect(issues[0].message).toContain('OrderSide');
    expect(issues[0].line).toBe(1);
  });

  it('rejects default, namespace and side-effect imports', () => {
    expect(checkRuntimeImports(`import x from '@tealstreet/script-api-core';`)).toHaveLength(1);
    expect(checkRuntimeImports(`import * as x from '@tealstreet/script-api-core';`)).toHaveLength(1);
    expect(checkRuntimeImports(`import '@tealstreet/script-api-core';`)).toHaveLength(1);
  });

  it('rejects re-exports and dynamic imports', () => {
    expect(checkRuntimeImports(`export { OrderSide } from '@tealstreet/script-api-core';`)).toHaveLength(1);
    expect(checkRuntimeImports(`export * from '@tealstreet/script-api-core';`)).toHaveLength(1);
    expect(
      checkRuntimeImports(`export default async function () { await import('@tealstreet/script-api-core'); }`),
    ).toHaveLength(1);
  });

  it('allows a type-only re-export', () => {
    expect(checkRuntimeImports(`export type { CoreApi } from '@tealstreet/script-api-core';`)).toEqual([]);
  });

  // A script does export its default function — that must not be flagged.
  it('does not flag a local export with no module specifier', () => {
    expect(checkRuntimeImports(`export default async function () {}`)).toEqual([]);
  });

  // `fileExists` constrains module resolution but NOT a reference directive,
  // which asks the host for an arbitrary path directly. Untrusted source
  // reaches this validator over MCP and a web route, so an arbitrary-file-read
  // here would be a genuine vulnerability, not a tidiness issue.
  it('blocks arbitrary file reads via a reference directive', () => {
    const marker = path.join(os.tmpdir(), `script-authoring-escape-${process.pid}.ts`);
    fs.writeFileSync(marker, 'export type Leaked = { pwned: true };\n');
    try {
      const escape = `/// <reference path="${marker}" />\nexport default async function () { const x: Leaked = { pwned: true }; void x; }`;
      const result = validateScript(escape, options);
      // If the file had been read, `Leaked` would resolve and this would pass.
      expect(result.valid).toBe(false);
      expect(result.errors.some((e) => e.code === 6053)).toBe(true);
      expect(result.errors.some((e) => e.code === 2304)).toBe(true);
    } finally {
      fs.unlinkSync(marker);
    }
  });

  it('flags only the value specifiers in a mixed import', () => {
    const issues = checkRuntimeImports(`import { type CoreApi, OrderSide } from '@tealstreet/script-api-core';`);
    expect(issues).toHaveLength(1);
    expect(issues[0].message).toContain('OrderSide');
  });
});

describe('validate', () => {
  it('composes both passes into one result', () => {
    const source = `import { OrderSide } from '@tealstreet/script-api-core';\n${wrap('  const n: number = "x";')}`;
    const result = validate(source, options);
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.code === 2322)).toBe(true);
    expect(result.errors.some((e) => e.message.includes('OrderSide'))).toBe(true);
  });

  it('passes a clean script', () => {
    expect(validate(wrap('  void api;\n  void args;'), options).valid).toBe(true);
  });
});
