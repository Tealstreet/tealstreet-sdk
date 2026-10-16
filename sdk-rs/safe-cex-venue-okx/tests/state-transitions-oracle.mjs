import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const url=new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts',import.meta.url);
const ast=ts.createSourceFile(url.pathname,fs.readFileSync(url,'utf8'),ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='OKXExchange');
const members=['fetchAccountMarketOverlay','getOrFetchPositionMode','getOrFetchAccountLevel','changeMarginType'].map(name=>{
  const member=owner.members.find(n=>n.name?.getText(ast)===name);
  assert.ok(member,'maintained state method '+name);return member.getText(ast);
});
const module={exports:{}};
const code=ts.transpileModule('export class Selected {'+members.join('\n')+'}',{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
vm.runInThisContext('(function(module,exports){'+code+'\n})')(module,module.exports);
const input=JSON.parse(fs.readFileSync(0,'utf8'));
assert.equal(input.projections.length,144,'independent projection floor');
const projections=[];
for(const c of input.projections){
  const receiver=new module.exports.Selected(),calls=[],failure=new TypeError('state callback failure');
  let resolve,reject,returned=false;
  receiver.getOrFetchPositionModeAndAccLevel=force=>{calls.push(force);return new Promise((r,j)=>{resolve=r;reject=j;});};
  const method=c.kind==='mode'?'getOrFetchPositionMode':'getOrFetchAccountLevel';
  const bits=Buffer.from(c.levelBits,'hex');
  const pending=receiver[method](c.force).then(value=>{returned=true;if(c.kind==='mode')return {mode:value};const b=Buffer.alloc(8);b.writeDoubleBE(value);return {bits:b.toString('hex')};},e=>{returned=true;return {error:{name:e.name,message:e.message,same:e===failure}};});
  assert.deepEqual(calls,[c.force??false],'force forwarded once before await');
  assert.equal(returned,false,'projection awaits dependency');
  if(c.fail)reject(failure);else resolve({positionMode:c.mode,accountLevel:bits.readDoubleBE()});
  projections.push({calls,outcome:await pending});
}
const receiver=new module.exports.Selected();
const overlay=await receiver.fetchAccountMarketOverlay();
assert.equal(overlay,null,'explicit null overlay');
const warnings=[];
for(const fail of [false,true]){
  const messages=[],failure=new TypeError('state callback failure');
  receiver.emitAndLogError=text=>{messages.push(text);if(fail)throw failure;};
  let error=null;
  try{await receiver.changeMarginType(true,String.fromCharCode(0xd800));}catch(e){error={name:e.name,message:e.message,same:e===failure};}
  warnings.push({messages,error});
}
process.stdout.write(JSON.stringify({projections,overlay,warnings}));
