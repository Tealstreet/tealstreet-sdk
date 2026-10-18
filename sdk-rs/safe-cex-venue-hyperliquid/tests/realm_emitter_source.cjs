const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
const compiler = require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const isolated = createRequire(compiler);
const ts = isolated(compiler);
const {TinyEmitter} = isolated('tiny-emitter');
const filename = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/base.ts');
const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'BaseExchange');
assert(owner, 'maintained BaseExchange');
const initializer = name => {
  const member = owner.members.find(node => node.name?.getText(ast) === name);
  assert(member?.initializer, `maintained ${name} initializer`);
  return member.initializer.getText(ast);
};
const receiver = {name:'hyperliquid', options:{tuning:{consoleLogging:false}}};
const context = vm.createContext({TinyEmitter, LogSeverity:{Info:'Info'}, console, receiver});
receiver.emitter = vm.runInContext(initializer('emitter'), context);
const log = vm.runInContext(ts.transpileModule(`function install(){return (${initializer('log')});} install.call(receiver)`, {
  compilerOptions:{target:ts.ScriptTarget.ES2022},
}).outputText, context);
const rows = [];
for (const event of ['arity','warning','log','numbers']) receiver.emitter.on(event, (...args) => rows.push({event,args}));
receiver.emitter.emit('arity');
const array = [7,'eight'], metadata = {code:42};
receiver.emitter.emit('arity', array);
receiver.emitter.emit('warning','venue warning',metadata);
log('native log','Warning');
const numbers = [NaN, Infinity, -Infinity, -0, 7, 42];
receiver.emitter.emit('numbers',...numbers);
assert.equal(rows.length,5);
assert.deepEqual(rows.map(({event,args})=>({event,json:JSON.stringify(args)})), [
  {event:'arity',json:'[]'},
  {event:'arity',json:'[[7,"eight"]]'},
  {event:'warning',json:'["venue warning",{"code":42}]'},
  {event:'log',json:'["[hyperliquid] native log","Warning"]'},
  {event:'numbers',json:'[null,null,null,0,7,42]'},
]);
assert.equal(rows[1].args[0],array);
assert.equal(rows[2].args[1],metadata);
array[0]=8; metadata.code=43;
assert.equal(rows[1].args[0][0],8);
assert.equal(rows[2].args[1].code,43);
array[0]=7; metadata.code=42;
for(let i=0;i<numbers.length;i++) assert(Object.is(rows[4].args[i],numbers[i]));
const bits = value => {
  const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
process.stdout.write(JSON.stringify({
  rows:rows.map(({event,args})=>({event,json:JSON.stringify(args)})),
  bits:[bits(array[0]),bits(metadata.code),...numbers.map(bits)],
  sourceIdentity:true,
}));
