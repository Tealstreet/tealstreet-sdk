import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runInNewContext } from 'node:vm';

import ts from 'typescript';
import { describe, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { BETA_VENUES } from '../../exchanges/rust/betaVenues';
import { createRustRegistry } from '../../exchanges/rustRegistry';

const loadRegistry = (file: string) => {
  class RustEngineExchange {}
  const runtime = vi.fn(() => ({ RustEngineExchange }));
  const path = resolve(process.cwd(), 'src/exchanges', file);
  const compiled = ts.transpileModule(readFileSync(path, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
  }).outputText;
  const exports: { exchangeRegistry?: ReturnType<typeof createRustRegistry> } = {};
  runInNewContext(compiled, {
    exports,
    require: (specifier: string) => {
      if (specifier === './rustRegistry') return { createRustRegistry };
      if (specifier === './rust/rust.exchange') return runtime();
      throw new Error(`Unexpected runtime dependency: ${specifier}`);
    },
  });
  return { registry: exports.exchangeRegistry!, runtime, RustEngineExchange };
};

describe('React Native and web Rust-only registry parity', () => {
  it.each(['registry.ts', 'registry.native.ts'])('loads only Rust for every IN name: %s', async (file) => {
    const { registry, runtime, RustEngineExchange } = loadRegistry(file);
    for (const name of BETA_VENUES) expect(await registry[name]()).toBe(RustEngineExchange);
    expect(runtime).toHaveBeenCalledTimes(BETA_VENUES.length);
  });
  it.each(['registry.ts', 'registry.native.ts'])('refuses every OUT name before runtime loading: %s', async (file) => {
    const { registry, runtime } = loadRegistry(file);
    for (const name of Object.keys(exchangeMetadata) as Array<keyof typeof exchangeMetadata>) {
      if (BETA_VENUES.some((venue) => venue === name)) continue;
      await expect(registry[name]()).rejects.toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
    }
    expect(runtime).not.toHaveBeenCalled();
  });
});
