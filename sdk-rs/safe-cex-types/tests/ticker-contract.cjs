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
const exact: Equal<Ticker, PublicTicker> = true;
const times: Equal<NonNullable<Ticker['observedAt']>, [number | null, number | null, number | null]> = true;
const alias: Equal<NonNullable<Ticker['observedAt']>, TickerObservedAt> = true;
const nullable: Ticker['observedAt'] = [0, null, 1.5];
const omitted: Pick<Ticker, 'observedAt' | 'midPrice'> = {};
// @ts-expect-error Receipt time tuple has exactly three entries.
const short: Ticker['observedAt'] = [0, null];
// @ts-expect-error Receipt time tuple rejects textual time.
const text: Ticker['observedAt'] = ['0', null, 1];
// @ts-expect-error Stored midpoint is numeric when present.
const clear: Ticker['midPrice'] = null;
// @ts-expect-error Ticker fields are readonly.
generated.last = 4;
// @ts-expect-error The optional observation field is readonly.
generated.observedAt = [0, 0, 0];
// Tuple elements retain the existing mutable tuple contract.
generated.observedAt![0] = null;
`;
  assert.ok(fs.existsSync(path.join(models, 'Ticker.ts')), 'Rust-generated public Ticker must exist');
  const configFile = path.join(root, 'packages/safe-cex/tsconfig.json');
  const config = ts.readConfigFile(configFile, ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configFile));
  const host = ts.createCompilerHost(parsed.options);
  const original = host.getSourceFile.bind(host);
  host.getSourceFile = (file, version, ...args) => file === fixture
    ? ts.createSourceFile(file, source, version, true) : original(file, version, ...args);
  const program = ts.createProgram([fixture], { ...parsed.options, incremental: false }, host);
  assert.deepEqual(ts.getPreEmitDiagnostics(program).map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')), []);
});

test('the public Ticker bytes are generated from the isolated Rust slice', () => {
  const { execFileSync } = require('node:child_process');
  const { transform } = require('../generate.cjs');
  const raw = execFileSync('cargo', ['run', '--locked', '--manifest-path', path.join(root, 'sdk-rs/Cargo.toml'),
    '-p', 'safe-cex-types', '--example', 'generate_ticker', '-j2', '--quiet'], { encoding: 'utf8' });
  const generated = transform(raw, {}, 'Ticker.ts').source;
  assert.equal(fs.readFileSync(path.join(models, 'Ticker.ts'), 'utf8'), generated);
});
