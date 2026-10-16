const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

let source = fs.readFileSync(
  path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts'),
  'utf8',
);
if (process.env.ASTERDEX_PUBLIC_KLINE_CONTROL === 'swap-close') {
  const original = 'close: parseFloat(kline.c)';
  if (!source.includes(original)) throw Error('handleKlineUpdate source control target missing');
  source = source.replace(original, 'close: parseFloat(kline.o)');
}
const ast = ts.createSourceFile('public-ws.ts', source, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const member = cls.members.find(n => n.name?.getText(ast) === 'handleKlineUpdate');
if (!member) throw Error('Missing handleKlineUpdate');
const method = ts.createPrinter().printNode(ts.EmitHint.Unspecified, member, ast);
const compiled = ts.transpileModule(
  'export class AsterDEXPublicWebsocket {' + method + '}',
  { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } },
).outputText;
const output = { exports: {} };
new Function('exports', 'module', compiled)(output.exports, output);
const numberBits = value => {
  if (Number.isNaN(value)) return '7ff8000000000000';
  if (process.env.ASTERDEX_KLINE_CAPTURE_CONTROL === 'erase-zero' && Object.is(value, -0)) return '0000000000000000';
  if (process.env.ASTERDEX_KLINE_CAPTURE_CONTROL === 'erase-finite' && Number.isFinite(value) && value !== 0) return '0000000000000000';
  const bytes = Buffer.allocUnsafe(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
const cases = [
  { name: 'all-fields', input: { s: 'BTCUSDT', k: { s: 'IGNORED', i: '1m', t: 1700000000000, o: '10.25x', h: '14.5', l: '8.75', c: '12.125', v: '20.5' } }, callbackStreams: ['btcusdt@kline_1m'] },
  { name: 'fallback-symbol', input: { s: '', k: { s: 'ETHUSDT', i: '5m', t: 1700000000123, o: '1', h: '4', l: '0.5', c: '2', v: '8' } }, callbackStreams: ['ethusdt@kline_5m'] },
  { name: 'signed-zero-and-nan', input: { s: 'ZERO', k: { i: '1m', t: -0, o: '-0', h: 'Infinity', l: '-Infinity', c: 'NaN', v: '1.25' } }, nativeNegativeZero: 'k.t', callbackStreams: ['zero@kline_1m'] },
  { name: 'negative-zero-timestamp-lexeme', inputText: '{"s":"ZERO","k":{"i":"1m","t":-0,"o":"1","h":"2","l":"0.5","c":"1.5","v":"3"}}', nativeNegativeZero: 'k.t', callbackStreams: ['zero@kline_1m'] },
  { name: 'missing-interval', input: { k: { s: 'ABC', o: '1', h: '2', l: '0.5', c: '1.5', v: '3' } }, callbackStreams: ['abc@kline_undefined'] },
  { name: 'no-callback', input: { s: 'BTCUSDT', k: { i: '1m', t: 1, o: '1', h: '2', l: '0.5', c: '1.5', v: '3' } }, callbackStreams: [] },
  { name: 'missing-kline', input: { s: 'BTCUSDT' }, callbackStreams: ['btcusdt@kline_1m'] },
  { name: 'null-kline', input: { s: 'BTCUSDT', k: null }, callbackStreams: ['btcusdt@kline_1m'] },
  { name: 'unicode-lowercase', input: { s: 'İΣ', k: { i: '1m', t: 2, o: '1', h: '3', l: '0.5', c: '2', v: '4' } }, callbackStreams: ['i̇ς@kline_1m'] },
  { name: 'numeric-symbol-error', input: { s: 42, k: { i: '1m' } }, callbackStreams: ['42@kline_1m'] },
  { name: 'null-data-error', input: null, callbackStreams: [] },
];
for (const c of cases) {
  const input = c.inputText ? JSON.parse(c.inputText) : c.input;
  const trace = [];
  const instance = new output.exports.AsterDEXPublicWebsocket();
  instance.klineCallbacks = new Map(c.callbackStreams.map(stream => [stream, candle => trace.push({ stream, candle })]));
  try {
    instance.handleKlineUpdate(input);
    c.result = trace.map(({ stream, candle }) => ({
      stream,
      timestamp: candle.timestamp === undefined
        ? { kind: 'undefined' }
        : typeof candle.timestamp === 'number'
          ? { kind: 'number', bits: numberBits(candle.timestamp) }
          : { kind: 'value', value: candle.timestamp },
      openBits: numberBits(candle.open),
      highBits: numberBits(candle.high),
      lowBits: numberBits(candle.low),
      closeBits: numberBits(candle.close),
      volumeBits: numberBits(candle.volume),
    }));
  } catch (error) {
    c.error = { name: error?.name, message: error?.message };
  }
}
process.stdout.write(JSON.stringify({
  captureArchitecture: `${process.platform}-${process.arch}`,
  captureNodeMajor: Number(process.versions.node.split('.')[0]),
  cases,
}));
