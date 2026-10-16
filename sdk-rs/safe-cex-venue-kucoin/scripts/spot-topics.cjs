const fs = require('node:fs'), path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root} = require('./tables.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoinspot.ws-public.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'KucoinSpotPublicWebsocket');
const members = new Map(['subscribeTopic','unsubscribeTopic'].map(name => {
 const member = owner.members.find(n => n.name?.getText(ast) === name);
 if (!member?.initializer) throw Error('missing maintained '+name);
 return [name, member.initializer.getText(ast)];
}));
function capture(c) {
 const events = [], frames = [], failure = new TypeError('topic send failure');
 const receiver = {isConnected:c.connected, isDisposed:c.disposed, send(frame) {
  events.push('send');frames.push(frame);if (c.fail) throw failure;
 }};
 const date = {now(){events.push('date');return c.date;}};
 const module = {exports:{}};
 const code = ts.transpileModule('module.exports='+members.get(c.method), {
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
 }).outputText;
 new Function('module','Date',code).call(receiver,module,date);
 let error = null;
 try {module.exports(c.topic);}catch(e){error={name:e.name,message:e.message,same:e===failure};}
 return {events,frames,error};
}
if (process.argv.includes('--surrogate-witness')) {
 const assert = require('node:assert/strict');
 const rows = ['subscribeTopic','unsubscribeTopic'].map(method => {
  const captured = capture({method,topic:String.fromCharCode(0xd800),connected:true,disposed:false,fail:false,date:123});
  const decoded = JSON.parse(captured.frames[0]);
  assert.equal(decoded.topic.length,1,'materialized topic length');
  assert.equal(decoded.topic.charCodeAt(0),0xd800,'materialized topic unit after transmission');
  return {method,units:[decoded.topic.charCodeAt(0)],...captured};
 });
 process.stdout.write(JSON.stringify(rows));
 process.exit(0);
}
const cases = JSON.parse(fs.readFileSync(0,'utf8'));
if (cases.length !== 48) throw Error('declared spot topic profile floor');
const captures = cases.map(capture);
process.stdout.write(JSON.stringify(captures));
