const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');

const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.utils.ts');
const sourceText = fs.readFileSync(file, 'utf8');
const source = ts.createSourceFile(file, sourceText, ts.ScriptTarget.Latest, true);
const names = new Set(['base64UrlEncode', 'base64UrlEncodeString', 'asn1Wrap', 'pemToPkcs8Der']);
const declarations = source.statements.filter((statement) =>
  ts.isFunctionDeclaration(statement) && names.has(statement.name?.text ?? ''),
);
assert.equal(declarations.length, names.size, 'all crypto encoding helpers must exist');
const exportsObject = {};
const compiled = ts.transpileModule(
  `${declarations.map((declaration) => declaration.getText(source)).join('\n')}\nexport { base64UrlEncode, base64UrlEncodeString, asn1Wrap, pemToPkcs8Der };`,
  { compilerOptions: { module: ts.ModuleKind.CommonJS } },
).outputText;
vm.runInNewContext(compiled, { exports: exportsObject, ArrayBuffer, Uint8Array, TextEncoder, atob, btoa });

const binaryInputs = [
  [],
  [0],
  [0xfb, 0xff, 0xf0],
  Array.from({ length: 127 }, (_, index) => (index * 37) & 0xff),
  Array.from({ length: 128 }, (_, index) => (index * 37) & 0xff),
  Array.from({ length: 256 }, (_, index) => (index * 37) & 0xff),
];
const binary = binaryInputs.flatMap((bytes, index) => [
  { label: `uint8-${index}`, bytes, result: exportsObject.base64UrlEncode(Uint8Array.from(bytes)) },
  { label: `array-buffer-${index}`, bytes, result: exportsObject.base64UrlEncode(Uint8Array.from(bytes).buffer) },
]);
const strings = ['', 'ascii', '\u0000', 'é', '中', '💰', 'line\nfeed', '\uFEFFbom'];
const stringCases = strings.map((input) => ({ input, result: exportsObject.base64UrlEncodeString(input) }));
const lengthCases = [0, 1, 127, 128, 255, 256, 257, 65536].map((length) => {
  const data = Uint8Array.from({ length }, (_, index) => (index * 19) & 0xff);
  return { label: `length-${length}`, tag: 4, bytes: Array.from(data), result: Array.from(exportsObject.asn1Wrap(4, data)) };
});
const tagCases = [4, 48, -1, 256.9, 1e20, null].map((value) => {
  const tag = value === null ? Number.NaN : value;
  const data = Uint8Array.from([1, 2]);
  return { label: `tag-${String(value)}`, tag: value === null ? 'NaN' : value, bytes: [1, 2], result: Array.from(exportsObject.asn1Wrap(tag, data)) };
});
const pemInputs = [
  ...[0, 1, 32, 129].map((length) => ({
    label: `pkcs8-${length}`,
    input: `-----BEGIN PRIVATE KEY-----\n${Buffer.from(Array.from({ length }, (_, index) => (index * 23) & 0xff)).toString('base64')}\n-----END PRIVATE KEY-----`,
  })),
  ...[0, 1, 64, 128].map((length) => ({
    label: `sec1-${length}`,
    input: `-----BEGIN EC PRIVATE KEY-----\n${Buffer.from(Array.from({ length }, (_, index) => (index * 29) & 0xff)).toString('base64')}\n-----END EC PRIVATE KEY-----`,
  })),
  { label: 'whitespace', input: '-----BEGIN PRIVATE KEY-----\uFEFF Z g = = \n-----END PRIVATE KEY-----' },
  { label: 'no-padding', input: 'Zm8' },
  { label: 'noncanonical-trailing-bits', input: 'Zh==' },
  { label: 'invalid-one-symbol', input: 'A' },
  { label: 'invalid-character', input: 'Zm?8' },
];
const pem = pemInputs.map(({ label, input }) => {
  try {
    return { label, input, result: Array.from(new Uint8Array(exportsObject.pemToPkcs8Der(input))) };
  } catch {
    return { label, input, result: null };
  }
});

process.stdout.write(JSON.stringify({ binary, strings: stringCases, lengths: lengthCases, tags: tagCases, pem }));
