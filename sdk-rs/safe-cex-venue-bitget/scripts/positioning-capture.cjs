const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BitgetExchange');
const methods = ['getOrFetchPositionMode', 'getOrFetchMarginType'];
const members = methods.map(name => {
  const member = klass.members.find(n => n.name?.getText(ast) === name);
  assert(member && ts.isMethodDeclaration(member), `POSITIONING actual method ${name}`);
  return member.getText(ast);
});
const domainPath = path.join(root, 'packages/safe-cex/src/types.ts');
const domain = ts.createSourceFile(domainPath, fs.readFileSync(domainPath, 'utf8'), ts.ScriptTarget.Latest, true);
const enums = ['PositionMode', 'MarginType'].map(name => {
  const node = domain.statements.find(n => ts.isEnumDeclaration(n) && n.name.text === name);
  assert(node, `POSITIONING actual enum ${name}`);
  return node.getText(domain).replace(/^export\s+/, '');
});
const source = enums.join('\n') + `\nclass Source {
  declare usesUtaRouting: boolean;
  declare resolveSymbolPositioning: (symbol:string) => Promise<{positionMode?:PositionMode; marginType?:MarginType}>;
  declare emitAndLogError: (message:string) => void;
  ${members.join('\n')}
}`;
const virtual = path.join(root, 'sdk-rs/safe-cex-venue-bitget/scripts/positioning-runtime.ts');
const options = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, strict: true, types: [], skipLibCheck: true };
const host = ts.createCompilerHost(options);
const original = host.getSourceFile.bind(host);
host.getSourceFile = (name, language, ...args) => name === virtual ? ts.createSourceFile(name, source, language, true) : original(name, language, ...args);
const program = ts.createProgram([virtual], options, host);
const diagnostics = ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length, 0, `POSITIONING source diagnostics: ${ts.formatDiagnosticsWithColorAndContext(diagnostics, {
  getCurrentDirectory: () => root, getCanonicalFileName: f => f, getNewLine: () => '\n',
})}`);
let emitted;
program.emit(undefined, (name, text) => { if (name.endsWith('positioning-runtime.js')) emitted = text; });
assert(emitted, 'POSITIONING source emit');
const Source = new Function(emitted + ';return Source;')();
(async () => {
  const cases = [];
  for (const method of methods) for (const symbol of ['', 'BTCUSDT', 'Σ😀', '\ufeff'])
    for (const uta of [false, true]) for (const mode of ['first', 'second', 'missing', 'reject']) for (const reportFails of [false, true]) {
      const input = { method, symbol, uta, mode, reportFails, sibling: 'throw' };
      const receiver = new Source(), calls = [];
      const field = method === methods[0] ? 'positionMode' : 'marginType';
      const other = field === 'positionMode' ? 'marginType' : 'positionMode';
      const values = field === 'positionMode' ? ['hedged', 'oneway'] : ['isolated', 'cross'];
      const resolverError = Object.assign(new Error('resolver failure'), { name: 'ResolverFailure' });
      const reporterError = Object.assign(new Error('reporter failure'), { name: 'ReporterFailure' });
      receiver.resolveSymbolPositioning = async arg => {
        calls.push(['resolve', arg]);
        if (mode === 'reject') throw resolverError;
        return { get [field]() { calls.push(['field', field]); return mode === 'missing' ? undefined : values[mode === 'first' ? 0 : 1]; },
          get [other]() { throw Error('POSITIONING sibling getter must not execute'); } };
      };
      Object.defineProperty(receiver, 'usesUtaRouting', { get() { calls.push(['uta']); return uta; } });
      receiver.emitAndLogError = message => { calls.push(['report', message]); if (reportFails) throw reporterError; };
      let output;
      try { output = { value: await receiver[method](symbol) }; }
      catch (error) { output = { error: { name: error.name, message: error.message, sameResolver: error === resolverError, sameReporter: error === reporterError } }; }
      cases.push({ input, calls, output });
    }
  const boundaries = [];
  for (const method of methods) for (const mode of ['open', 'null-object', 'field-throw', 'surrogate']) {
    const receiver = new Source(), calls = [], field = method === methods[0] ? 'positionMode' : 'marginType';
    const symbol = mode === 'surrogate' ? '\ud800' : 'BOUNDARY';
    receiver.resolveSymbolPositioning = async () => mode === 'null-object' ? null : mode === 'field-throw' ?
      { get [field]() { throw new TypeError('selected getter failure'); } } : { [field]: mode === 'open' ? 'OPEN' : undefined };
    Object.defineProperty(receiver, 'usesUtaRouting', { get() { calls.push('uta'); return false; } });
    receiver.emitAndLogError = message => calls.push(JSON.stringify(message));
    let output;
    try { output = { value: await receiver[method](symbol) }; }
    catch (error) { output = { error: { name: error.name, message: JSON.stringify(error.message) } }; }
    boundaries.push({ method, mode, symbol: JSON.stringify(symbol), calls, output });
  }
  await new Promise((resolve, reject) => process.stdout.write(JSON.stringify({
    provenance: { architecture: process.arch, node: process.version }, cases, boundaries,
  }) + '\n', error => error ? reject(error) : resolve()));
})().catch(error => { console.error(error); process.exitCode = 1; });
