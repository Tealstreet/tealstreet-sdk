const assert = require('node:assert/strict');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const source = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types.ts');
const probe = path.join(__dirname, '../tests/literal-rejection-probe.ts');
// These are the exact surviving pair and a distinguishing positive, not a new
// fixture capture or a claim that their values/diagnostic messages are identical.
const raw = ['""', 'null', '"unifiedAccount"'];
const annotation = process.argv.includes('--control-widened') ? 'any' : 'Source.HyperliquidUserAbstractionMode';
const text = 'import type * as Source from "../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types";\n'
  + raw.map((value, index) => `const probe${index}: ${annotation} = ${value};`).join('\n');
const options = {strict:true, noEmit:true, skipLibCheck:true, types:[], target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS};
const host = ts.createCompilerHost(options);
const read = host.readFile.bind(host), exists = host.fileExists.bind(host);
host.readFile = file => file === probe ? text : read(file);
host.fileExists = file => file === probe || exists(file);
const program = ts.createProgram([source, probe], options, host);
const diagnostics = ts.getPreEmitDiagnostics(program).filter(diagnostic => diagnostic.file?.fileName === probe);
assert.ok(!diagnostics.some(diagnostic => diagnostic.code === 2307), 'LITERAL-SOURCE actual declaration import resolves');
const ast = program.getSourceFile(probe);
const declarations = ast.statements.filter(ts.isVariableStatement);
assert.equal(declarations.length, 3, 'LITERAL-SOURCE fixed pair and positive inventory');
const cases = declarations.map((declaration, index) => {
  const errors = diagnostics.filter(diagnostic => diagnostic.start >= declaration.pos && diagnostic.start < declaration.end);
  return {raw:raw[index], accepted:errors.length === 0, codes:errors.map(error => error.code), messages:errors.map(error => ts.flattenDiagnosticMessageText(error.messageText, '\n'))};
});
assert.deepEqual(cases.map(row => row.accepted), [false, false, true], 'LITERAL-SOURCE rejected pair and distinguishing positive');
assert.deepEqual(cases.map(row => row.codes), [[2322], [2322], []], 'LITERAL-SOURCE assignability diagnostics');
assert.notEqual(cases[0].messages[0], cases[1].messages[0], 'LITERAL-SOURCE diagnostic text distinction retained');
console.log(JSON.stringify({type:'HyperliquidUserAbstractionMode', observable:'strict declared-type assignability only; values and diagnostic messages differ', cases}));
