const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/backpack');
const parse = name => ts.createSourceFile(name, fs.readFileSync(path.join(root, name), 'utf8'), ts.ScriptTarget.Latest, true);
const api = parse('backpack.api.ts');
let interceptor;
function visit(node) {
  if (ts.isCallExpression(node) && node.expression.getText(api) === 'xhr.interceptors.response.use') interceptor = node.arguments[0].getText(api);
  ts.forEachChild(node, visit);
}
visit(api);
assert.ok(interceptor, 'maintained response interceptor');
const types = parse('backpack.types.ts');
const endpoints = types.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(types) === 'ENDPOINTS'));
assert.ok(endpoints);
const exchange = parse('backpack.exchange.ts');
const cls = exchange.statements.find(ts.isClassDeclaration);
const member = cls.members.find(n => n.name?.getText(exchange) === 'fetchMarkets');
assert.ok(member);
const context = vm.createContext({exports:{}, LogSeverity:{Error:'error'}, appendHttpFailure(){throw Error('unexpected source error branch');}});
const compile = text => ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInContext(compile(endpoints.getText(types).replace(/^export /, '') + `\nexports.intercept=(${interceptor}); exports.Subject=class {${member.getText(exchange)}};`),context);
(async()=>{
  const raw = fs.readFileSync(0,'utf8');
  const data = JSON.parse(raw);
  const subject = new context.exports.Subject();
  subject.xhr={get:async()=>context.exports.intercept({data})};
  subject.log=(message)=>{console.error(message);};
  const rows=await subject.fetchMarkets();
  const units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
  process.stdout.write(JSON.stringify(rows.map(row=>({symbol:row.symbol,opaque:units(row.info.opaque),keys:Object.keys(row.info).map(units)}))));
})().catch(error=>{console.error(error);process.exitCode=1;});
