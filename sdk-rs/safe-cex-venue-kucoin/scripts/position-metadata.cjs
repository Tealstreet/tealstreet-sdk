// Live futures metadata wrappers; their shared position dependency is injected.
const fs = require('node:fs'), path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root} = require('./tables.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoin.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'KucoinExchange');
const names = ['getOrFetchPositionMode', 'getOrFetchMarginType'];
function bind(name, context) {
 const member = cls.members.find(n => n.name?.getText(ast) === name);
 if (!member || !ts.isMethodDeclaration(member)) throw Error('missing live ' + name);
 const expression = member.getText(ast).replace(/^async\s+\w+/, 'async function');
 const module = {exports:{}};
 const code = ts.transpileModule('module.exports=' + expression, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 new Function('module', code).call(context, module);
 return module.exports.bind(context);
}
(async () => {
 const captures = [];
 for (const c of JSON.parse(fs.readFileSync(0, 'utf8'))) {
  if (c.symbolRaw !== undefined) c.symbol = JSON.parse(c.symbolRaw);
  const calls = [], out = {}, failure = new TypeError('position dependency failure');
  const context = {getOrFetchPositionsForSymbol:async symbol => {
   calls.push(symbol);
   if (c.failure) throw failure;
   return c.positions;
  }};
  for (const name of names) {
   try { out[name] = {ok:await bind(name, context)(c.symbol)}; }
   catch (error) { out[name] = {message:error.message,name:error.name,sameFailure:error === failure}; }
  }
  captures.push(c.symbolRaw !== undefined ? {outJson:JSON.stringify({calls,...out})} : {calls,...out});
 }
 console.log(JSON.stringify(captures));
})().catch(error => {console.error(error);process.exitCode=1;});
