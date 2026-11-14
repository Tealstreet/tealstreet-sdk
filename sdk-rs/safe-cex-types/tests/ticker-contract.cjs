const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const ts = require('typescript');
const root = path.resolve(__dirname, '../../..');
const models = path.join(root, 'packages/safe-cex/src/generated');

test('generated Ticker preserves the public contract including timestamp tuple and readonly fields', () => {
  const fixture = path.join(models, '__ticker_contract_check__.ts');
  const source = `import type { Ticker } from './Ticker';
import type { Ticker as PublicTicker, TickerObservedAt } from '../types';
declare const generated: Ticker;
declare const publicTicker: PublicTicker;
const a: PublicTicker = generated;
const b: Ticker = publicTicker;
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
const keys: Equal<keyof Ticker, keyof PublicTicker> = true;
const exact: Equal<Omit<Ticker, 'info'>, Omit<PublicTicker, 'info'>> = true;
const metadata: Equal<Ticker['info'], unknown> = true;
const times: Equal<NonNullable<Ticker['observedAt']>, [number | null, number | null, number | null]> = true;
const alias: Equal<NonNullable<Ticker['observedAt']>, TickerObservedAt> = true;
const nullable: Ticker['observedAt'] = [0, null, 1.5];
const omitted: Pick<Ticker, 'observedAt' | 'midPrice'> = {};
if (generated.observedAt) generated.observedAt[0] = null;
`;
  assert.ok(fs.existsSync(path.join(models, 'Ticker.ts')), 'Rust-generated public Ticker must exist');
  const configFile = path.join(root, 'packages/safe-cex/tsconfig.json');
  const config = ts.readConfigFile(configFile, ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configFile));
  const host = ts.createCompilerHost(parsed.options);
  const original = host.getSourceFile.bind(host);
  const negatives = [
    { name: 'short-tuple', code: 2322, source: "const value: Ticker['observedAt'] = [0, null];" },
    { name: 'text-time', code: 2322, source: "const value: Ticker['observedAt'] = ['0', null, 1];" },
    { name: 'stored-null-midpoint', code: 2322, source: "const value: Ticker['midPrice'] = null;" },
    { name: 'readonly-price', code: 2540, source: 'generated.last = 4;' },
    { name: 'readonly-observation', code: 2540, source: 'generated.observedAt = [0, 0, 0];' },
    { name: 'unvalidated-info', code: 18046, source: 'generated.info.quote;' },
  ];
  const inputs = new Map([[fixture, source]]);
  for (const negative of negatives) {
    negative.file = path.join(models, `__ticker_${negative.name}__.ts`);
    inputs.set(negative.file, `import type { Ticker } from './Ticker';\ndeclare const generated: Ticker;\n${negative.source}`);
  }
  host.getSourceFile = (file, version, ...args) => inputs.has(file)
    ? ts.createSourceFile(file, inputs.get(file), version, true) : original(file, version, ...args);
  const program = ts.createProgram([...inputs.keys()], { ...parsed.options, incremental: false }, host);
  const diagnostics = ts.getPreEmitDiagnostics(program);
  const expectedFiles = new Set(negatives.map(negative => negative.file));
  assert.deepEqual(diagnostics.filter(d => !expectedFiles.has(d.file?.fileName))
    .map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')), []);
  for (const negative of negatives) {
    assert.deepEqual(diagnostics.filter(d => d.file?.fileName === negative.file).map(d => d.code), [negative.code], negative.name);
  }
});

test('the public Ticker bytes are generated from the isolated Rust slice', () => {
  const { execFileSync } = require('node:child_process');
  const { transform } = require('../generate.cjs');
  const raw = execFileSync('cargo', ['run', '--locked', '--manifest-path', path.join(root, 'sdk-rs/Cargo.toml'),
    '-p', 'safe-cex-types', '--example', 'generate_ticker', '-j2', '--quiet'], { encoding: 'utf8' });
  const generated = transform(raw, {}, 'Ticker.ts').source;
  assert.equal(fs.readFileSync(path.join(models, 'Ticker.ts'), 'utf8'), generated);
});


test('public Ticker directly re-exports the Rust-generated declaration', () => {
  const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/types.ts'), 'utf8');
  assert.doesNotMatch(source, /export\s+(?:type|interface)\s+Ticker\s*[={]/);
  assert.match(source, /import type \{ Ticker \} from '\.\/generated\/Ticker';/);
});
