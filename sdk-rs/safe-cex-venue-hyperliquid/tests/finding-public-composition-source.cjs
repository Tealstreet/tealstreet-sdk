'use strict';
// Later composition supplement; original seven-case feeder remains unchanged.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const root=process.env.SAFE_CEX_ROOT || path.resolve(__dirname,'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript',{paths:[root]}));
const file='packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-public.ts';
const ast=ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='HyperliquidPublicWebSocket');assert(klass,'PUBLIC composition actual class');
const calls=[];let completed=0;
const context={exports:{},flatten:require(require.resolve('lodash/flatten',{paths:[root,path.dirname(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'))]})),BaseWebSocket:class {constructor(parent){this.parent=parent;}dispose(){calls.push(['super.dispose',this]);return {ignored:true};}}};
vm.runInNewContext(ts.transpileModule(klass.getText(ast)+';exports.Class=HyperliquidPublicWebSocket;',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
const C=context.exports.Class,owner=new C({}),other=new C({}),sent=[];
owner.sendWithDiagnostics=text=>sent.push(text);
const nativeInputs=[],originalSender=owner.sendSubscription;
owner.sendSubscription=input=>{nativeInputs.push(input);return originalSender(input);};
owner.topics={'candle.BTC.1m':1,'l2Book.ETH':1,'ignored.X':1};
const fixed=owner.subscribePublicTopics;assert.equal(fixed.call(other),undefined);
assert.equal(owner.activeSubscriptions.size,3,'PUBLIC composed source owner Set');
assert.equal(other.activeSubscriptions.size,0,'PUBLIC composed source lexical receiver');
assert.equal(sent.length,5,'PUBLIC composed independent source sender floor');
assert.equal(nativeInputs.length,3,'PUBLIC composed source independent native argument floor');
for(const [i,input] of nativeInputs.entries()){
  const keys=i===0?['type','coin','interval']:['type','coin'];
  assert.deepEqual(Reflect.ownKeys(input),keys,'PUBLIC composed source exact own argument keys '+i);
  for(const key of keys)assert.equal(Object.getOwnPropertyDescriptor(input,key).enumerable,true,'PUBLIC composed source enumerable argument '+i);
}
assert.deepEqual(sent.map(s=>JSON.parse(s).subscription),[{type:'allMids'},{type:'fastAssetCtxs'},{type:'candle',coin:'BTC',interval:'1m'},{type:'l2Book',coin:'ETH'},{type:'bbo',coin:'ETH'}],'PUBLIC composed source payload identities');
completed++;
other.stopPingLoop=()=>calls.push(['stopPingLoop',other]);
assert.equal(C.prototype.dispose.call(other),undefined);
assert.equal(calls.length,2,'PUBLIC installed source dispose independent effect floor');
for(const [i,name] of ['stopPingLoop','super.dispose'].entries()){assert.equal(calls[i][0],name,'PUBLIC installed source dispose order');assert.equal(calls[i][1],other,'PUBLIC installed source dispose dynamic receiver');}
completed++;assert.equal(completed,2,'PUBLIC composition independent completed source floor');
process.stdout.write(JSON.stringify({cases:completed,stop:'installed-dispose-dynamic-receiver',purpose:'later composition supplement; no original independence claim'})+'\n');
