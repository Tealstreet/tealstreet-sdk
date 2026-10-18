import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test, type TestContext } from 'node:test';
import ts from 'typescript';
import { extractSources, isSource, manifestPathForSource, sourcePrefix, walkFiles, type SourceSymbols } from '../src/extract.ts';
import { parseManifest, reconcile, serializeManifest, type Manifest } from '../src/manifest.ts';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../../../..');
const fixtureSource = sourcePrefix + 'example.ts';

function write(path: string, content: string) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}

function fixture(t: TestContext) {
  const root = mkdtempSync(join(tmpdir(), 'parity-extract-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  write(join(root, 'packages/safe-cex/tsconfig.json'), JSON.stringify({
    compilerOptions: { target: 'ES2022', strict: true, skipLibCheck: true, module: 'ESNext', moduleResolution: 'bundler', types: [] },
    include: ['src/**/*.ts'], exclude: ['src/excluded-by-config.ts'],
  }));
  return { root, manifestRoot: join(root, 'sdk-rs/parity/manifest') };
}

function snapshot(directory: string) {
  return existsSync(directory) ? walkFiles(directory).map((path) => ({
    path, text: readFileSync(path, 'utf8'), mtime: statSync(path).mtimeMs,
  })) : [];
}

function annotatedManifest(): Manifest {
  return {
    file: { ts: fixtureSource },
    symbol: [{ ts: 'existing', kind: 'function', layer: 'adapted', rs: 'crate/src/lib.rs::existing',
      status: 'divergent', note: 'Keeps "quotes", """triples""", \\slashes,\ttabs,\nnewlines, Unicode λ and \u0000 controls.' }],
  };
}

test('parseManifest returns plain objects independent of parser internals', () => {
  const original = annotatedManifest();
  original.file = { ...original.file, rust_files: ['crate/src/lib.rs'] };
  const manifest = parseManifest(serializeManifest(original), fixtureSource);
  assert.equal(Object.getPrototypeOf(manifest.file), Object.prototype);
  assert.equal(Object.getPrototypeOf(manifest.symbol[0]), Object.prototype);
  assert.deepEqual(manifest, original);
});

test('Program resolves exported declarations and callable fields, in source order', (t) => {
  const { root } = fixture(t);
  write(join(root, fixtureSource), `
    import { imported } from './helper';
    const internal = () => 0;
    function local(value: string): string;
    function local(value: number): number;
    function local(value: string | number) { return value; }
    export { local as renamed };
    export { imported };
    export { imported as reexported } from './helper';
    export * from './helper';
    export const arrow = async () => 1, data = 1;
    export const expression = function () { return 1; };
    export const alias = local;
    export const wrapped = wrap(arrow);
    export const erased: any = ((() => 1) as any);
    function wrap<T>(value: T): T { return value; }
    export let mutable = () => 1;
    export const { destructured } = { destructured: () => 1 };
    export type Wire = { price: number };
    export interface Domain { id: string }
    export enum Side { Buy, Sell }
    class Hidden { hidden() {} }
    export class Venue extends Hidden {
      plain = 1;
      fetch = async () => [];
      optional?: (() => void) | null;
      declared!: (event: string) => void;
      wrapped = wrap(() => 1);
      private secret = () => 1;
      #hiddenField = () => 1;
      private secretMethod() {}
      #hiddenMethod() {}
      protected protectedMethod() {}
      static staticMethod() {}
      method(value: string): string;
      method(value: number): number;
      method(value: string | number) { return value; }
      ['literal']() {}
      erased: any = function () {};
      constructor(public tick: () => void, private readonly cancel: () => void, private count: number) { super(); }
      get value() { return 1; }
      set value(_v: number) {}
    }
    export default function () {}
  `);
  write(join(root, sourcePrefix, 'helper.ts'), 'export function imported() {}');
  write(join(root, sourcePrefix, 'excluded-by-config.ts'), 'export const included = () => 1;');
  const files = extractSources(root);
  assert.deepEqual(files.find((file) => file.ts === fixtureSource)?.symbols, [
    ['local', 'function'], ['arrow', 'const-fn'], ['data', 'const'], ['expression', 'const-fn'],
    ['alias', 'const-fn'], ['wrapped', 'const-fn'], ['erased', 'const-fn'], ['wrap', 'function'], ['mutable', 'variable-fn'], ['destructured', 'const-fn'],
    ['Wire', 'type'], ['Domain', 'type'], ['Side', 'enum'], ['Hidden', 'class'], ['Hidden.hidden', 'method'], ['Venue', 'class'],
    ['Venue.fetch', 'field-fn'], ['Venue.optional', 'field-fn'], ['Venue.declared', 'field-fn'],
    ['Venue.wrapped', 'field-fn'], ['Venue#secret', 'field-fn'], ['Venue.#hiddenField', 'field-fn'],
    ['Venue#secretMethod', 'method'], ['Venue.#hiddenMethod', 'method'],
    ['Venue.protectedMethod', 'method'], ['Venue.static staticMethod', 'method'],
    ['Venue.method', 'method'], ["Venue.['literal']", 'method'],
    ['Venue.erased', 'field-fn'], ['Venue.constructor', 'constructor'],
    ['Venue.tick', 'field-fn'], ['Venue#cancel', 'field-fn'],
    ['Venue.get value', 'accessor'], ['Venue.set value', 'accessor'],
    ['default', 'function'],
  ].map(([ts, kind]) => ({ ts, kind })));
  assert.deepEqual(files.find((file) => file.ts.endsWith('excluded-by-config.ts'))?.symbols,
    [{ ts: 'included', kind: 'const-fn' }]);
});

test('contract exclusions apply even when imported by included source', (t) => {
  const { root } = fixture(t);
  const excluded = ['a.test.ts', 'a.test.tsx', '__tests__/helper.ts', '__snapshots__/a.ts',
    'nested/__tests__/a.ts', 'env.d.ts', 'another.d.ts', 'nested/perfFixture.ts'];
  for (const path of excluded) {
    assert.equal(isSource(path), false, path);
    write(join(root, sourcePrefix, path), 'export const omitted = () => 1;');
  }
  write(join(root, fixtureSource), `import './a.test'; export function included() {}`);
  assert.deepEqual(extractSources(root).map((file) => file.ts), [fixtureSource]);
});

test('static, instance, TS-private and native-private names remain distinct', (t) => {
  const { root } = fixture(t);
  write(join(root, fixtureSource), `export class Venue {
    static call() {} private call() {} #call() {}
    static field = () => 1; field = () => 2; #field = () => 3;
    static get value() { return 1; } get value() { return 2; }
    get #value() { return 3; } set #value(_value: number) {}
    static #secret() {}
  }`);
  assert.deepEqual(extractSources(root)[0].symbols.map((row) => row.ts), [
    'Venue', 'Venue.static call', 'Venue#call', 'Venue.#call',
    'Venue.static field', 'Venue.field', 'Venue.#field',
    'Venue.static get value', 'Venue.get value', 'Venue.get #value', 'Venue.set #value', 'Venue.static #secret',
  ]);
});

test('reachable classes include singleton, factory and subclass behavior, excluding disconnected siblings', (t) => {
  const { root } = fixture(t);
  write(join(root, fixtureSource), `
    class Singleton { now = () => 1; }
    class Product { run() {} }
    class Base { base() {} }
    class Disconnected { hidden() {} }
    const instance = new Singleton(), unrelated = new Disconnected();
    export { instance };
    const helper = () => new Product();
    export function factory() { return helper(); }
    export class Subclass extends Base {}
    export function throwsError() { throw new ModuleError(); }
    class ModuleError extends Error {}
    class Shorthand { call() {} }
    const member = new Shorthand();
    export const provider = { member };
  `);
  assert.deepEqual(extractSources(root)[0].symbols.map((row) => row.ts), [
    'Singleton', 'Singleton.now', 'Product', 'Product.run', 'Base', 'Base.base',
    'instance', 'helper', 'factory', 'Subclass', 'throwsError', 'ModuleError', 'Shorthand', 'Shorthand.call', 'provider',
  ]);
});

test('exported object providers emit value declarations and function properties, excluding data properties and unexported providers', (t) => {
  const { root } = fixture(t);
  write(join(root, fixtureSource), `
    const shorthand = () => 1;
    const internal = { unused() {} };
    export const provider = ({
      data: 1,
      arrow: () => 1,
      async load() { return 1; },
      shorthand,
      expression: function () {},
      nested: { data: 2 },
    } as const);
  `);
  assert.deepEqual(extractSources(root)[0].symbols, [{ ts: 'shorthand', kind: 'const-fn' }, { ts: 'provider', kind: 'const' }, ...[
    'provider.arrow', 'provider.load', 'provider.shorthand', 'provider.expression',
  ].map((ts) => ({ ts, kind: 'field-fn' }))]);
});

test('unsupported provider shapes fail before any manifest writes while plain data remains legal', (t) => {
  const { root, manifestRoot } = fixture(t);
  const path = join(manifestRoot, 'example.ts.toml');
  write(path, serializeManifest(annotatedManifest()));
  for (const [shape, code] of [
    ['Object.freeze', 'export const provider = Object.freeze({ run() {} });'],
    ['class expression', 'export const provider = class { run() {} };'],
    ['nested callable object', 'export const provider = { inner: { deep() {} } };'],
    ['callable spread', 'const base = { run() {} }; export const provider = { ...base };'],
  ]) {
    write(join(root, fixtureSource), code);
    const before = snapshot(manifestRoot);
    assert.throws(() => reconcile(extractSources(root), manifestRoot, false), (error: unknown) => {
      assert.ok(error instanceof Error);
      assert.ok(error.message.includes('example.ts') && error.message.includes('provider') && error.message.includes(shape), error.message);
      return true;
    });
    assert.deepEqual(snapshot(manifestRoot), before);
  }
  write(join(root, fixtureSource), `const data = { value: 1 };
    export const frozen = Object.freeze({ value: 2, list: [1, 2] });
    export const array = Object.freeze([1, 2]);
    export const spreadArray = { ...[1, 2] };
    export const provider = { ...data, nested: { value: 3, list: [1, 2] }, run() { return { inner() {} }; } };`);
  assert.deepEqual(extractSources(root)[0].symbols, [
    ...['frozen', 'array', 'spreadArray', 'provider'].map((ts) => ({ ts, kind: 'const' })),
    { ts: 'provider.run', kind: 'field-fn' },
  ]);
});

test('reachable private functions include transitive helpers but exclude disconnected declarations', (t) => {
  const { root } = fixture(t);
  write(join(root, fixtureSource), `
    const leaf = () => 1;
    const alias = leaf;
    function helper() { return alias(); }
    function disconnected() { return unused(); }
    const unused = () => 2;
    export function entry() { return helper(); }
  `);
  assert.deepEqual(extractSources(root)[0].symbols, [
    { ts: 'leaf', kind: 'const-fn' }, { ts: 'alias', kind: 'const-fn' },
    { ts: 'helper', kind: 'function' }, { ts: 'entry', kind: 'function' },
  ]);
});

test('module initialization registrations retain callable helpers', (t) => {
  const { root } = fixture(t);
  write(join(root, sourcePrefix, 'service.ts'), 'export const service = { callback: () => 0 };');
  write(join(root, fixtureSource), `import { service } from './service';
    const registered = () => 1;
    service.callback = registered;
    export function entry() { return service.callback(); }`);
  assert.deepEqual(extractSources(root).find((file) => file.ts === fixtureSource)?.symbols, [
    { ts: 'registered', kind: 'const-fn' }, { ts: 'entry', kind: 'function' },
  ]);
});

test('changing TS-private to native-private preserves the annotated old name as an orphan', (t) => {
  const { root, manifestRoot } = fixture(t);
  write(join(root, fixtureSource), 'export class Venue { private call() {} }');
  reconcile(extractSources(root), manifestRoot, false);
  const path = join(manifestRoot, 'example.ts.toml');
  const original = parseManifest(readFileSync(path, 'utf8'), fixtureSource);
  const row = original.symbol.find((symbol) => symbol.ts === 'Venue#call')!;
  Object.assign(row, { status: 'divergent', layer: 'adapted', note: 'Keep migration context', rs: 'crate/src/lib.rs::call' });
  write(path, serializeManifest(original));
  write(join(root, fixtureSource), 'export class Venue { #call() {} }');
  const sources = extractSources(root);
  const before = snapshot(manifestRoot);
  const checked = reconcile(sources, manifestRoot, true);
  assert.deepEqual(checked.orphaned, [`${fixtureSource}::Venue#call (method)`]);
  assert.deepEqual(snapshot(manifestRoot), before);
  reconcile(sources, manifestRoot, false);
  const updated = parseManifest(readFileSync(path, 'utf8'), fixtureSource);
  assert.deepEqual(updated.symbol.find((symbol) => symbol.ts === 'Venue#call'), row);
  assert.deepEqual(updated.symbol.find((symbol) => symbol.ts === 'Venue.#call'), {
    ts: 'Venue.#call', kind: 'method', status: 'pending', layer: '', note: '', rs: '',
  });
});

test('externally declared callable handles are excluded while package-declared function types remain', (t) => {
  const { root } = fixture(t);
  write(join(root, 'node_modules/client/index.d.ts'), `export interface Client { (): number }
    export declare function makeClient(): Client;`);
  write(join(root, fixtureSource), `
    import { makeClient, type Client } from 'client';
    type Callback = () => number;
    interface Callable { (): number }
    export class Venue {
      handle = makeClient();
      optional?: Client;
      local?: Callback;
      callable?: Callable;
      own = () => makeClient();
      constructor(private readonly xhr: Client, private callback: Callback) {}
    }
  `);
  assert.deepEqual(extractSources(root)[0].symbols.map((row) => row.ts), [
    'Venue', 'Venue.local', 'Venue.callable', 'Venue.own', 'Venue.constructor', 'Venue#callback',
  ]);
});

test('wrapped local functions and delegated methods remain rows despite external callable types', (t) => {
  const { root } = fixture(t);
  write(join(root, 'node_modules/client/index.d.ts'), `
    export declare class ClientBase { get(): void }
    export interface Client extends ClientBase { (): number }
    export interface Wrapped { (): number; cancel(): void }
    export declare class Emitter { on(event: string): void }
    export declare function makeClient(callback?: () => number): Client;
    export declare function wrap(callback: () => number): Wrapped;
  `);
  write(join(root, fixtureSource), `
    import { Emitter, makeClient, wrap, type Client, type ClientBase, type Wrapped } from 'client';
    const callback = () => 1;
    const alias = callback;
    function namedCallback() { return 2; }
    export class Venue {
      direct: Wrapped = Object.assign(() => 1, { cancel() {} });
      wrapped = wrap(() => 1);
      nested = wrap(wrap(function () { return 2; }));
      referenced = wrap(alias);
      named = wrap(namedCallback);
      emitter = new Emitter();
      delegated = this.emitter.on;
      on: Emitter['on'];
      assigned: Wrapped;
      xhr: Client;
      opaqueClient: ClientBase;
      constructor() {
        this.on = this.emitter.on.bind(this.emitter);
        this.assigned = wrap(() => 1);
        this.xhr = makeClient();
        this.opaqueClient = makeClient(callback);
      }
    }
  `);
  assert.deepEqual(extractSources(root)[0].symbols.map((row) => row.ts), [
    'callback', 'alias', 'namedCallback', 'Venue', 'Venue.direct', 'Venue.wrapped', 'Venue.nested', 'Venue.referenced', 'Venue.named',
    'Venue.delegated', 'Venue.on', 'Venue.assigned', 'Venue.constructor',
  ]);
});

test('reconciliation preserves agent-owned annotations and is byte- and mtime-stable', (t) => {
  const { manifestRoot } = fixture(t);
  const original = annotatedManifest();
  const path = join(manifestRoot, 'example.ts.toml');
  write(path, serializeManifest(original));
  const sources: SourceSymbols[] = [{ ts: fixtureSource, symbols: [
    { ts: 'newOne', kind: 'const-fn' }, { ts: 'existing', kind: 'function' },
  ] }];
  assert.equal(reconcile(sources, manifestRoot, false).added.length, 1);
  const updated = parseManifest(readFileSync(path, 'utf8'), fixtureSource);
  assert.deepEqual(updated.symbol[1], original.symbol[0]);
  assert.deepEqual(updated.symbol[0], {
    ts: 'newOne', kind: 'const-fn', layer: '', rs: '', status: 'pending', note: '',
  });
  const before = snapshot(manifestRoot);
  assert.deepEqual(reconcile(sources, manifestRoot, false), { added: [], orphaned: [], changed: [] });
  assert.deepEqual(snapshot(manifestRoot), before);
  assert.deepEqual(reconcile(sources, manifestRoot, true), { added: [], orphaned: [], changed: [] });
  assert.deepEqual(snapshot(manifestRoot), before);
});

test('all statuses and layers survive reruns', (t) => {
  const { manifestRoot } = fixture(t);
  const manifest = annotatedManifest();
  const statuses = ['pending', 'ported', 'divergent', 'not-applicable'] as const;
  manifest.symbol = statuses.map((status) => ({ ...manifest.symbol[0], ts: status, status,
    layer: status === 'ported' ? 'mirror' : 'adapted', note: status === 'ported' ? '' : 'Keep this note' }));
  const path = join(manifestRoot, 'example.ts.toml');
  write(path, serializeManifest(manifest));
  reconcile([{ ts: fixtureSource, symbols: manifest.symbol }], manifestRoot, false);
  assert.deepEqual(parseManifest(readFileSync(path, 'utf8'), fixtureSource), manifest);
});

test('removed symbols and kind changes remain orphaned with annotations intact', (t) => {
  const { manifestRoot } = fixture(t);
  const manifest = annotatedManifest();
  const path = join(manifestRoot, 'example.ts.toml');
  write(path, serializeManifest(manifest));
  const sources: SourceSymbols[] = [{ ts: fixtureSource, symbols: [{ ts: 'existing', kind: 'const-fn' }] }];
  const result = reconcile(sources, manifestRoot, false);
  assert.deepEqual(result.orphaned, [`${fixtureSource}::existing (function)`]);
  assert.deepEqual(parseManifest(readFileSync(path, 'utf8'), fixtureSource).symbol[1], manifest.symbol[0]);
  const before = snapshot(manifestRoot);
  assert.equal(reconcile(sources, manifestRoot, true).orphaned.length, 1);
  assert.deepEqual(snapshot(manifestRoot), before);
});

test('deleted source manifests, including zero-symbol files, are retained and reported', (t) => {
  const { manifestRoot } = fixture(t);
  write(join(manifestRoot, 'example.ts.toml'), serializeManifest(annotatedManifest()));
  write(join(manifestRoot, 'empty.ts.toml'), serializeManifest({ file: { ts: sourcePrefix + 'empty.ts' }, symbol: [] }));
  const before = snapshot(manifestRoot);
  for (const check of [false, true]) {
    const result = reconcile([], manifestRoot, check);
    assert.equal(result.orphaned.length, 2);
    assert.match(result.orphaned.join('\n'), /existing/);
    assert.ok(result.orphaned.every((message) => message.includes('source file vanished')));
    assert.deepEqual(snapshot(manifestRoot), before);
  }
});

test('check does not create output directories or write reordered/new/orphaned rows', (t) => {
  const { manifestRoot } = fixture(t);
  const source: SourceSymbols = { ts: fixtureSource, symbols: [{ ts: 'newOne', kind: 'function' }] };
  assert.equal(reconcile([source], manifestRoot, true).added.length, 1);
  assert.equal(existsSync(manifestRoot), false);
  write(join(manifestRoot, 'example.ts.toml'), serializeManifest(annotatedManifest()));
  const before = snapshot(manifestRoot);
  const result = reconcile([source], manifestRoot, true);
  assert.equal(result.added.length, 1);
  assert.equal(result.orphaned.length, 1);
  assert.deepEqual(snapshot(manifestRoot), before);
});

test('malformed, mismatched, unknown, and duplicate metadata abort before any write', (t) => {
  const { manifestRoot } = fixture(t);
  const valid = serializeManifest(annotatedManifest());
  const badInputs = [
    '[file', valid.replace('example.ts', 'wrong.ts'), valid.replace('layer = "adapted"', 'layer = 1'),
    valid.replace('layer = "adapted"', 'layer = "other"'),
    valid + '\nunknown = "must not be discarded"\n',
    serializeManifest({ ...annotatedManifest(), symbol: [annotatedManifest().symbol[0], annotatedManifest().symbol[0]] }),
  ];
  for (const text of badInputs) {
    write(join(manifestRoot, 'example.ts.toml'), text);
    const before = snapshot(manifestRoot);
    assert.throws(() => reconcile([{ ts: sourcePrefix + 'a-new.ts', symbols: [] }], manifestRoot, false));
    assert.deepEqual(snapshot(manifestRoot), before);
  }
});

test('source syntax and tsconfig errors fail before reconciliation', (t) => {
  const { root } = fixture(t);
  write(join(root, fixtureSource), 'export function broken( {');
  assert.throws(() => extractSources(root));
  write(join(root, 'packages/safe-cex/tsconfig.json'), '{ invalid');
  assert.throws(() => extractSources(root));
});

test('unresolved type imports abort before reconciliation instead of silently omitting callable fields', (t) => {
  const { root, manifestRoot } = fixture(t);
  write(join(root, fixtureSource), `import type { Callback } from './missing';
    export class Venue { field!: Callback; }`);
  const result = spawnSync(process.execPath,
    ['--import', 'tsx', join(repoRoot, 'sdk-rs/parity/extract-ts/src/cli.ts'), '--repo-root', root],
    { cwd: repoRoot, encoding: 'utf8' });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /TS2307/);
  assert.match(result.stderr, /example.ts/);
  assert.equal(existsSync(manifestRoot), false);
  write(join(root, fixtureSource), `import type { Callback } from './helper';
    export class Venue { field!: Callback; }`);
  write(join(root, sourcePrefix, 'helper.ts'), `import type { Callback } from './missing'; export type { Callback };`);
  assert.throws(() => extractSources(root), /TS2307/);
});

test('CLI coverage gate exits nonzero and names additions and orphans without writing', (t) => {
  const { root, manifestRoot } = fixture(t);
  write(join(root, fixtureSource), 'export function fresh() {}');
  const invoke = (...args: string[]) => spawnSync(process.execPath,
    ['--import', 'tsx', join(repoRoot, 'sdk-rs/parity/extract-ts/src/cli.ts'), '--repo-root', root, ...args],
    { cwd: repoRoot, encoding: 'utf8' });
  const initial = invoke('--check');
  assert.equal(initial.status, 1, initial.stderr);
  assert.match(initial.stdout, /added: .*::fresh \(function\)/);
  assert.equal(existsSync(manifestRoot), false);
  assert.equal(invoke().status, 0);
  const before = snapshot(root);
  assert.equal(invoke('--check').status, 0);
  assert.deepEqual(snapshot(root), before);
  write(join(root, fixtureSource), 'export function renamed() {}');
  const changedBefore = snapshot(root);
  const changed = invoke('--check');
  assert.equal(changed.status, 1);
  assert.match(changed.stdout, /orphaned: .*::fresh \(function\)/);
  assert.match(changed.stdout, /added: .*::renamed \(function\)/);
  assert.deepEqual(snapshot(root), changedBefore);
  const written = invoke();
  assert.equal(written.status, 0, written.stderr);
  assert.match(written.stdout, /orphaned: .*::fresh/);
  assert.match(readFileSync(join(manifestRoot, 'example.ts.toml'), 'utf8'), /ts = "fresh"/);
  const bad = invoke('--unknown');
  assert.equal(bad.status, 1);
  assert.match(bad.stderr, /Unknown or incomplete argument/);
});

test('CLI names malformed manifest paths and format-only changes without writing', (t) => {
  const { root, manifestRoot } = fixture(t);
  write(join(root, fixtureSource), 'export function existing() {}');
  const path = join(manifestRoot, 'example.ts.toml');
  const invoke = () => spawnSync(process.execPath,
    ['--import', 'tsx', join(repoRoot, 'sdk-rs/parity/extract-ts/src/cli.ts'), '--repo-root', root, '--check'],
    { cwd: repoRoot, encoding: 'utf8' });
  write(path, '[file');
  const malformedBefore = snapshot(root);
  const malformed = invoke();
  assert.equal(malformed.status, 1);
  assert.ok(malformed.stderr.includes(path), malformed.stderr);
  assert.deepEqual(snapshot(root), malformedBefore);
  write(path, serializeManifest(annotatedManifest()) + '\n');
  const before = snapshot(root);
  const changed = invoke();
  assert.equal(changed.status, 1);
  assert.match(changed.stdout, /changed: packages\/safe-cex\/src\/example.ts/);
  assert.match(changed.stdout, /0 added, 0 orphaned/);
  assert.deepEqual(snapshot(root), before);
});

test('real safe-cex Program includes Bybit and BaseExchange callable instance fields', () => {
  const files = extractSources(repoRoot);
  const bybit = files.find((file) => file.ts === sourcePrefix + 'exchanges/bybit/bybit.exchange.ts');
  const base = files.find((file) => file.ts === sourcePrefix + 'exchanges/base.ts');
  for (const name of ['BybitExchange.fetchIncomeHistory', 'BybitExchange.placeOrder', 'BybitExchange.fetchPositions']) {
    assert.ok(bybit?.symbols.some((symbol) => symbol.ts === name && symbol.kind === 'field-fn'), name);
  }
  for (const name of ['BaseExchange.on', 'BaseExchange.once', 'BaseExchange.off', 'BaseExchange#startWatchdog']) {
    assert.ok(base?.symbols.some((symbol) => symbol.ts === name && symbol.kind === 'field-fn'), name);
  }
  assert.ok(base?.symbols.some((symbol) => symbol.ts === 'BaseExchange.rejectPreflight' && symbol.kind === 'method'));
  assert.ok(base?.symbols.some((symbol) => symbol.ts === 'BaseExchange.constructor' && symbol.kind === 'constructor'));
  assert.ok(base?.symbols.some((symbol) => symbol.ts === 'BaseExchange.get name' && symbol.kind === 'accessor'));
  const throttle = files.find((file) => file.ts === sourcePrefix + 'webworker/handlers/ThrottleManager.ts');
  for (const name of ['ThrottleManager#fn', 'ThrottleManager#getKey']) {
    assert.ok(throttle?.symbols.some((symbol) => symbol.ts === name && symbol.kind === 'field-fn'), name);
  }
  for (const [file, name, kind] of [
    ['utils/virtualClock.ts', 'VirtualClock.getCurrentTimeMs', 'field-fn'],
    ['webworker/twap/TwapManager.ts', 'TwapManager.add', 'method'],
    ['types.ts', 'OrderSide', 'enum'],
    ['transport/direct.ts', 'directTransport.rest', 'field-fn'],
  ]) {
    assert.ok(files.find((fileSymbols) => fileSymbols.ts === sourcePrefix + file)?.symbols
      .some((row) => row.ts === name && row.kind === kind), name);
  }
  for (const file of ['exchanges/registry.ts', 'exchanges/registry.native.ts']) {
    const source = ts.createSourceFile(file, readFileSync(join(repoRoot, sourcePrefix, file), 'utf8'), ts.ScriptTarget.Latest, true);
    const registry = source.statements.filter(ts.isVariableStatement)
      .flatMap((statement) => statement.declarationList.declarations)
      .find((declaration) => ts.isIdentifier(declaration.name) && declaration.name.text === 'exchangeRegistry');
    assert.ok(registry?.initializer && ts.isObjectLiteralExpression(registry.initializer), file);
    const expected = registry.initializer.properties.map((property) => {
      assert.ok(ts.isPropertyAssignment(property), file);
      assert.ok(ts.isIdentifier(property.name) || ts.isStringLiteral(property.name), file);
      return `exchangeRegistry.${property.name.text}`;
    });
    assert.ok(expected.includes('exchangeRegistry.binanceinverse'), file);
    const actual = files.find((source) => source.ts === sourcePrefix + file)?.symbols
      .filter((row) => row.ts.startsWith('exchangeRegistry.') && row.kind === 'field-fn').map((row) => row.ts);
    assert.deepEqual(actual?.sort(), expected.sort(), file);
  }
  const aster = files.find((source) => source.ts === sourcePrefix + 'exchanges/asterdex/asterdex.exchange.ts');
  assert.ok(!aster?.symbols.some((row) => row.ts === 'AsterDEXExchange.primaryAPI'));
  assert.ok(!bybit?.symbols.some((row) => row.ts === 'BybitExchange.xhr'));
  assert.ok(bybit?.symbols.some((row) => row.ts === 'BybitExchange#debouncedPrivateSnapshotRefresh'));
  const worker = files.find((source) => source.ts === sourcePrefix + 'webworker/ExchangeWorker.ts');
  assert.ok(worker?.symbols.some((row) => row.ts === 'ExchangeWorker#_throttledStoreUpdate'));
});

test('second root preserves annotations, source order, orphans and check write isolation', (t) => {
  const { root, manifestRoot } = fixture(t);
  const order = 'packages/script-api-core/src/order/example.ts';
  write(join(root, fixtureSource), 'export function existing() {}');
  write(join(root, order), 'export function existing() {}');
  write(join(root, 'packages/script-api-core/src/outside.ts'), 'export function outside() {}');
  write(join(root, 'packages/script-api-core/src/order/example.test.ts'), 'export function omitted() {}');
  const sources = extractSources(root);
  assert.deepEqual(sources.map((source) => source.ts), [fixtureSource, order]);
  assert.equal(reconcile(sources, manifestRoot, true).added.length, 2);
  assert.equal(existsSync(manifestRoot), false);
  reconcile(sources, manifestRoot, false);
  const path = join(manifestRoot, 'script-api-core/order/example.ts.toml');
  const annotated = { ...annotatedManifest(), file: { ts: order } };
  write(path, serializeManifest(annotated));
  const before = snapshot(manifestRoot);
  assert.deepEqual(reconcile(sources, manifestRoot, false), { added: [], orphaned: [], changed: [] });
  assert.deepEqual(snapshot(manifestRoot), before);
  assert.deepEqual(parseManifest(readFileSync(path, 'utf8'), order), annotated);
  const changed: SourceSymbols[] = [sources[0], { ts: order, symbols: [{ ts: 'existing', kind: 'const-fn' }] }];
  const checked = reconcile(changed, manifestRoot, true);
  assert.equal(checked.added.length, 1);
  assert.equal(checked.orphaned.length, 1);
  assert.deepEqual(snapshot(manifestRoot), before);
  reconcile(changed, manifestRoot, false);
  assert.deepEqual(parseManifest(readFileSync(path, 'utf8'), order).symbol[1], annotated.symbol[0]);
  const retained = snapshot(manifestRoot);
  assert.equal(reconcile([sources[0]], manifestRoot, false).orphaned.length, 2);
  assert.deepEqual(snapshot(manifestRoot), retained);
});

test('root mapping rejects collisions, path escapes, and malformed second-root metadata before writes', (t) => {
  const { manifestRoot } = fixture(t);
  for (const ts of [sourcePrefix + 'script-api-core/order/example.ts',
    'packages/script-api-core/src/outside.ts', 'packages/script-api-core/src/order/../escape.ts',
    'packages/script-api-core/src/order//example.ts', 'packages/script-api-core/src/order/./example.ts',
    'packages/script-api-core/src/order/example.test.ts', 'packages/script-api-core/src/order/a\\example.ts']) {
    assert.throws(() => reconcile([{ ts: fixtureSource, symbols: [] }, { ts, symbols: [] }], manifestRoot, false));
    assert.equal(existsSync(manifestRoot), false);
  }
  write(join(manifestRoot, 'script-api-core/order/example.ts.toml'), serializeManifest(annotatedManifest()));
  const before = snapshot(manifestRoot);
  assert.throws(() => reconcile([{ ts: fixtureSource, symbols: [] }], manifestRoot, false), /does not match/);
  assert.deepEqual(snapshot(manifestRoot), before);
});

test('array and string rs round-trip byte-identically through check and write reconciliation', (t) => {
  const { root, manifestRoot } = fixture(t);
  for (const rs of [
    ['zeta/src/lib.rs::Options', 'alpha/src/lib.rs::ExchangeOptions'],
    ['crate/src/lib.rs::ExchangeOptions'],
    'crate/src/lib.rs::ExchangeOptions',
  ]) {
    const manifest = annotatedManifest();
    manifest.symbol = [{ ...manifest.symbol[0], ts: 'ExchangeOptions', kind: 'type', rs }];
    const path = join(manifestRoot, 'example.ts.toml');
    const text = serializeManifest(manifest);
    write(path, text);
    write(join(root, fixtureSource), 'export interface ExchangeOptions { value: string }');
    const before = snapshot(manifestRoot);
    const cli = spawnSync(process.execPath,
      ['--import', 'tsx', join(repoRoot, 'sdk-rs/parity/extract-ts/src/cli.ts'), '--repo-root', root, '--check'],
      { cwd: repoRoot, encoding: 'utf8' });
    assert.equal(cli.status, 0, cli.stdout + cli.stderr);
    assert.match(cli.stdout, /Would update 0 manifests; 0 added, 0 orphaned symbols/);
    assert.deepEqual(snapshot(manifestRoot), before);
    const sources: SourceSymbols[] = [{ ts: fixtureSource, symbols: [{ ts: 'ExchangeOptions', kind: 'type' }] }];
    assert.deepEqual(parseManifest(text, fixtureSource), manifest);
    for (const check of [true, false]) {
      assert.deepEqual(reconcile(sources, manifestRoot, check), { added: [], orphaned: [], changed: [] });
      assert.deepEqual(snapshot(manifestRoot), before);
      assert.equal(readFileSync(path, 'utf8'), text);
    }
    sources[0].symbols.push({ ts: 'newOne', kind: 'function' });
    assert.equal(reconcile(sources, manifestRoot, false).added.length, 1);
    assert.deepEqual(parseManifest(readFileSync(path, 'utf8'), fixtureSource).symbol[0], manifest.symbol[0]);
    const updated = snapshot(manifestRoot);
    assert.deepEqual(reconcile(sources, manifestRoot, false), { added: [], orphaned: [], changed: [] });
    assert.deepEqual(snapshot(manifestRoot), updated);
  }
});

test('malformed rs arrays name the symbol and abort before reconciliation writes', (t) => {
  const { manifestRoot } = fixture(t);
  const manifest = annotatedManifest();
  manifest.symbol[0].ts = 'ExchangeOptions';
  const valid = serializeManifest(manifest);
  for (const rs of ['["crate/src/lib.rs::Options", 1]', '[true]', '[["nested"]]', '[{ item = "wrong" }]']) {
    write(join(manifestRoot, 'example.ts.toml'), valid.replace(/^rs = .*$/m, `rs = ${rs}`));
    const before = snapshot(manifestRoot);
    for (const check of [true, false]) {
      assert.throws(() => reconcile([{ ts: sourcePrefix + 'new.ts', symbols: [] }], manifestRoot, check),
        /packages\/safe-cex\/src\/example\.ts: ExchangeOptions rs must be a string or an array of strings/);
      assert.deepEqual(snapshot(manifestRoot), before);
    }
  }
});

test('per-file provenance survives reconciliation unchanged and rejects malformed declarations', (t) => {
  const { manifestRoot } = fixture(t);
  const path = join(manifestRoot, 'example.ts.toml');
  for (const rust_files of [undefined, [], ['zeta/src/lib.rs'], ['zeta/src/lib.rs', 'alpha/src/options.rs']]) {
    const manifest = annotatedManifest();
    if (rust_files !== undefined) manifest.file.rust_files = rust_files;
    const text = serializeManifest(manifest);
    write(path, text);
    const before = snapshot(manifestRoot);
    const sources: SourceSymbols[] = [{ ts: fixtureSource, symbols: [{ ts: 'existing', kind: 'function' }] }];
    assert.deepEqual(parseManifest(text, fixtureSource), manifest);
    for (const check of [true, false]) {
      assert.deepEqual(reconcile(sources, manifestRoot, check), { added: [], orphaned: [], changed: [] });
      assert.deepEqual(snapshot(manifestRoot), before);
    }
    sources[0].symbols.push({ ts: 'added', kind: 'function' });
    reconcile(sources, manifestRoot, false);
    assert.deepEqual(parseManifest(readFileSync(path, 'utf8'), fixtureSource).file, manifest.file);
  }
  const valid = serializeManifest(annotatedManifest());
  for (const value of ['"crate/src/lib.rs"', '[1]', '["crate/src/*.rs"]', '["crate/src/../lib.rs"]', '["crate/src/lib.rs::Item"]', '["crate/src/lib.rs", "crate/src/lib.rs"]']) {
    write(path, valid.replace(/^ts = .*$/m, (line) => `${line}\nrust_files = ${value}`));
    const before = snapshot(manifestRoot);
    assert.throws(() => reconcile([{ ts: fixtureSource, symbols: [] }], manifestRoot, false),
      /example\.ts: invalid or duplicate \[file\]\.rust_files/);
    assert.deepEqual(snapshot(manifestRoot), before);
  }
});

test('exported value constants cover policy, objects, instances, aliases and destructuring', (t) => {
  const { root } = fixture(t);
  write(join(root, fixtureSource), `
    const internal = 1;
    export const ALL_EXCHANGES = '*';
    export const TIMEOUT = 5000;
    export const data = { symbol: 'BTC' };
    export const list = ['BTC'];
    class Instance { call() { return internal; } }
    export const instance = new Instance();
    const aliased = ['ETH'];
    export { aliased as publicAlias };
    export const { first, second: renamed } = { first: 1, second: 2 };
    export const fn = () => internal;
  `);
  const rows = extractSources(root)[0].symbols;
  assert.deepEqual(rows.filter((row) => row.kind === 'const').map((row) => row.ts),
    ['ALL_EXCHANGES', 'TIMEOUT', 'data', 'list', 'instance', 'aliased', 'first', 'renamed']);
  assert.ok(rows.some((row) => row.ts === 'fn' && row.kind === 'const-fn'));
  assert.ok(rows.some((row) => row.ts === 'Instance.call' && row.kind === 'method'));
  assert.ok(!rows.some((row) => row.ts === 'internal'));
  assert.ok(!rows.some((row) => row.ts === 'data.symbol'));
});

test('CLI coverage rejects a missing exported data constant without writing', (t) => {
  const { root, manifestRoot } = fixture(t);
  write(join(root, fixtureSource), "export const ALL_EXCHANGES = '*';");
  const path = join(manifestRoot, 'example.ts.toml');
  write(path, serializeManifest({ file: { ts: fixtureSource }, symbol: [] }));
  const before = snapshot(manifestRoot);
  const cli = () => spawnSync(process.execPath,
    ['--import', 'tsx', join(repoRoot, 'sdk-rs/parity/extract-ts/src/cli.ts'), '--repo-root', root, '--check'],
    { cwd: repoRoot, encoding: 'utf8' });
  const missing = cli();
  assert.equal(missing.status, 1, missing.stdout + missing.stderr);
  assert.match(missing.stdout, /added: packages\/safe-cex\/src\/example\.ts::ALL_EXCHANGES \(const\)/);
  assert.match(missing.stdout, /1 added, 0 orphaned symbols/);
  assert.deepEqual(snapshot(manifestRoot), before);
  assert.equal(reconcile(extractSources(root), manifestRoot, false).added.length, 1);
  const clean = cli();
  assert.equal(clean.status, 0, clean.stdout + clean.stderr);
  assert.deepEqual(parseManifest(readFileSync(path, 'utf8'), fixtureSource).symbol,
    [{ ts: 'ALL_EXCHANGES', kind: 'const', layer: '', rs: '', status: 'pending', note: '' }]);
});

test('P3b runtime backend roots reconcile with disjoint prefixes', (t) => {
  const { root, manifestRoot } = fixture(t);
  write(join(root, fixtureSource), 'export {};');
  const packages = ['safe-cex-web', 'safe-cex-server', 'safe-cex-react-native'];
  for (const name of packages) {
    write(join(root, `packages/${name}/src/candleCache/Backend.ts`), 'export class Backend { constructor() {} loadChunks = async () => []; }');
  }
  const sources = extractSources(root);
  assert.equal(sources.length, 4);
  for (const source of sources.filter((source) => source.ts !== fixtureSource)) assert.deepEqual(source.symbols.map(({ ts, kind }) => [ts, kind]), [
    ['Backend', 'class'], ['Backend.constructor', 'constructor'], ['Backend.loadChunks', 'field-fn'],
  ]);
  const result = reconcile(sources, manifestRoot, false);
  assert.equal(result.added.length, 9);
  for (const name of packages) assert.ok(existsSync(join(manifestRoot, `${name}/candleCache/Backend.ts.toml`)));
  assert.equal(reconcile(extractSources(root), manifestRoot, true).changed.length, 0);
});

test('exported let and var bindings retain mutability, aliases, destructuring and callable providers', (t) => {
  const { root } = fixture(t);
  write(join(root, fixtureSource), `
    let privateState = 1;
    let privateCallable = () => privateState;
    export let token: string | undefined;
    export var socket = { close() {} };
    let local = 1; export { local as aliased };
    export let { first, second: renamed } = { first: 1, second: 2 };
    export var [item] = [1];
    export let callback = () => privateCallable();
    export var otherCallback = function () {};
  `);
  assert.deepEqual(extractSources(root)[0].symbols, [
    ['token', 'variable'], ['socket', 'variable'], ['socket.close', 'field-fn'],
    ['local', 'variable'], ['first', 'variable'], ['renamed', 'variable'], ['item', 'variable'],
    ['callback', 'variable-fn'], ['otherCallback', 'variable-fn'],
  ].map(([ts, kind]) => ({ ts, kind })));
});

for (const binding of ['let', 'var']) {
  test(`CLI coverage refuses missing exported ${binding} without writing; reconcile preserves annotations`, (t) => {
    const { root, manifestRoot } = fixture(t);
    write(join(root, fixtureSource), `export ${binding} state = 1;`);
    const path = join(manifestRoot, 'example.ts.toml');
    write(path, serializeManifest({ file: { ts: fixtureSource }, symbol: [] }));
    const before = snapshot(manifestRoot);
    const cli = () => spawnSync(process.execPath,
      ['--import', 'tsx', join(repoRoot, 'sdk-rs/parity/extract-ts/src/cli.ts'), '--repo-root', root, '--check'],
      { cwd: repoRoot, encoding: 'utf8' });
    const missing = cli();
    assert.equal(missing.status, 1, missing.stdout + missing.stderr);
    assert.match(missing.stdout, /state \(variable\)/);
    assert.deepEqual(snapshot(manifestRoot), before);
    assert.equal(reconcile(extractSources(root), manifestRoot, false).added.length, 1);
    const manifest = parseManifest(readFileSync(path, 'utf8'), fixtureSource);
    assert.deepEqual(manifest.symbol, [{ ts: 'state', kind: 'variable', layer: '', rs: '', status: 'pending', note: '' }]);
    manifest.symbol[0].note = 'Mutable module state; lifetime mapping still pending.';
    write(path, serializeManifest(manifest));
    const annotated = snapshot(manifestRoot);
    assert.equal(cli().status, 0);
    reconcile(extractSources(root), manifestRoot, false);
    assert.deepEqual(snapshot(manifestRoot), annotated);
  });
}

test('Lighter SDK closure preserves signer paths and inventories clients, types, constants, utilities and default API', (t) => {
  const { root, manifestRoot } = fixture(t);
  write(join(root, fixtureSource), 'export {};');
  write(join(root, 'packages/poseidon-crypto/src/Primitive.ts'), 'export function primitive() { return 1; }');
  const sdkSources = [
    ['signer/Primitive.ts', 'export function primitive() { return 1; }'],
    ['client/Client.ts', 'export class Client { send() {} }'],
    ['constants/index.ts', 'export const CHAIN = 304;'],
    ['types/Order.ts', 'export type Order = { id: bigint };'],
    ['utils/check.ts', 'export function check() { return true; }'],
    ['index.ts', 'const SDK = { version: 1 }; export default SDK;'],
  ] as const;
  for (const [name, source] of sdkSources) write(join(root, 'packages/lighter-sdk/src', name), source);
  write(join(root, 'packages/lighter-sdk/src/client/Client.test.ts'), 'export const excludedTest = 1;');
  write(join(root, 'packages/lighter-sdk/outside.ts'), 'export const excludedOutside = 1;');
  const sources = extractSources(root);
  assert.equal(sources.length, 8);
  assert.ok(!sources.some(source => /Client\.test|outside/.test(source.ts)));
  const result = reconcile(sources, manifestRoot, false);
  assert.equal(result.added.length, 8);
  assert.ok(existsSync(join(manifestRoot, 'poseidon-crypto/Primitive.ts.toml')));
  for (const [name] of sdkSources) assert.ok(existsSync(join(manifestRoot, 'lighter-sdk', name + '.toml')), name);
  const unchanged = reconcile(extractSources(root), manifestRoot, true);
  assert.equal(unchanged.changed.length, 0);
  assert.equal(unchanged.added.length, 0);
  assert.equal(unchanged.orphaned.length, 0);
});

test('json-parse root inventories both shared exports without widening past src', (t) => {
  const { root, manifestRoot } = fixture(t);
  const source = 'packages/json-parse/src/jsonParse.ts';
  write(join(root, fixtureSource), 'export {};');
  write(join(root, source), 'export const jsonParse = (s: string) => s; export const bigIntAsStringJsonParse = (s: string) => s;');
  write(join(root, 'packages/json-parse/outside.ts'), 'export const outside = 1;');
  write(join(root, 'packages/json-parse/src/jsonParse.test.ts'), 'export const omitted = 1;');
  const sources = extractSources(root);
  assert.deepEqual(sources.map((file) => file.ts), [source, fixtureSource]);
  assert.deepEqual(sources[0].symbols.map((row) => row.ts), ['jsonParse', 'bigIntAsStringJsonParse']);
  assert.equal(reconcile(sources, manifestRoot, true).added.length, 2);
  assert.equal(existsSync(manifestRoot), false);
  reconcile(sources, manifestRoot, false);
  const path = join(manifestRoot, 'json-parse/jsonParse.ts.toml');
  assert.equal(parseManifest(readFileSync(path, 'utf8'), source).file.ts, source);
  assert.throws(() => manifestPathForSource(sourcePrefix + 'json-parse/jsonParse.ts'));
});
