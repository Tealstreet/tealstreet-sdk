// Execute the four live venue ping bodies; clocks and send operations are seams.
const fs = require('node:fs'), path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root} = require('./tables.cjs');
const methods = new Map();
for (const venue of ['kucoin', 'kucoinspot']) {
 for (const role of ['public', 'private']) {
  const file = path.join(root, `packages/safe-cex/src/exchanges/kucoin/${venue}.ws-${role}.ts`);
  const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = ast.statements.find(ts.isClassDeclaration);
  const ping = cls.members.find(n => n.name?.getText(ast) === 'ping');
  if (!ping?.initializer) throw Error('missing live ping ' + file);
  methods.set(`${venue}-${role}`, ping.initializer.getText(ast));
 }
}
function bits(value) {
 const buffer = Buffer.alloc(8);buffer.writeDoubleBE(value);return buffer.toString('hex');
}
const cases = JSON.parse(fs.readFileSync(0, 'utf8'));
const captures = cases.map(c => {
 const events = [], frames = [], error = new TypeError('native send failure');
 const context = {isDisposed:c.disposed,pingAt:0};
 const send = frame => {events.push('send');frames.push(frame);if (c.fail) throw error;};
 context.send = send;context.sendWithLog = send;
 const date = {now:() => {events.push('date');return c.date;}};
 const performance = {now:() => {events.push('performance');return c.monotonic;}};
 const module = {exports:{}};
 const code = ts.transpileModule('module.exports=' + methods.get(c.method), {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 new Function('module','Date','performance', code).call(context, module, date, performance);
 let rejected;
 try {module.exports();}catch(caught) {rejected={message:caught.message,name:caught.name,same:caught===error};}
 return {events,frames,pingAtBits:bits(context.pingAt),...(rejected?{rejected}:{})};
});
console.log(JSON.stringify(captures));
