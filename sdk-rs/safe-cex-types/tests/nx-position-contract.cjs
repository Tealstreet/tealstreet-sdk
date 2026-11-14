const assert = require('node:assert/strict');
const path = require('node:path');
const test = require('node:test');
const ts = require('typescript');

const root = path.resolve(__dirname, '../../..');
const sdk = path.join(root, 'packages/safe-cex');
const config = ts.readConfigFile(path.join(sdk, 'tsconfig.json'), ts.sys.readFile);
assert.equal(config.error, undefined);
const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, sdk);
assert.deepEqual(parsed.errors, []);

// Effective language options from NX apps/web at d08627b49f6e. Workspace
// aliases and ambient dependencies come from this SDK checkout, so CI does
// not need an NX tree. NX's inherited strict:true enables strictNullChecks.
const nx = ts.convertCompilerOptionsFromJson(require('./fixtures/nx-position-compiler.json'), sdk);
assert.deepEqual(nx.errors, []);
const options = { ...parsed.options, ...nx.options };

function diagnostics(program) {
  return ts.getPreEmitDiagnostics(program).map(d => ({
    file: d.file?.fileName,
    code: d.code,
    line: d.file && d.start !== undefined ? d.file.getLineAndCharacterOfPosition(d.start).line + 1 : undefined,
    message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
  }));
}

test('actual FrameApplier and Position reader compile with NX strict language options', () => {
  const entry = path.join(sdk, 'src/store/frameApplier.ts');
  const program = ts.createProgram([entry], options);
  assert(program.getSourceFile(path.join(sdk, 'src/store/positionFrames.ts')), 'compile the real Position reader');
  assert.deepEqual(diagnostics(program), []);
});

test('NX nullability gate catches all five old Position reader errors', () => {
  const file = path.join(sdk, 'src/store/__nx_position_negative_control__.ts');
  const source = `import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
declare const positions: NonNullable<ReturnType<Frame['positions']>>;
declare const i: number;
declare function finite(value: number): number | undefined;
const oldReader = {
  liquidationPrice: positions.liquidationPrice[i] > 0 ? (finite(positions.liquidationPrice[i]) ?? null) : null,
  ...(finite(positions.realizedPnl[i]) !== undefined && { realizedPnl: positions.realizedPnl[i] }),
  ...(finite(positions.marginSize[i]) !== undefined && { marginSize: positions.marginSize[i] }),
  ...(finite(positions.lastUpdate[i]) !== undefined && { lastUpdate: positions.lastUpdate[i] }),
};
`;
  const host = ts.createCompilerHost(options);
  const original = host.getSourceFile.bind(host);
  host.getSourceFile = (name, version, ...args) => name === file
    ? ts.createSourceFile(name, source, version, true)
    : original(name, version, ...args);
  const errors = diagnostics(ts.createProgram([file], options, host));
  assert.deepEqual(errors.map(({ file: filename, code, line }) => ({ file: filename, code, line })), [
    { file, code: 2531, line: 6 },
    { file, code: 2345, line: 6 },
    { file, code: 2345, line: 7 },
    { file, code: 2345, line: 8 },
    { file, code: 2345, line: 9 },
  ]);
});
