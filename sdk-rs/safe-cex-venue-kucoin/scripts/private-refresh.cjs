const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/kucoin');
function body(file,ownerName,names){const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===ownerName);return names.map(name=>{const m=owner.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,'maintained '+name);return m.getText(ast);}).join('\n');}
const methods={snapshot:body(path.join(root,'kucoin.ws-private.ts'),'KucoinPrivateWebsocket',['refreshSnapshotSoon']),spot:body(path.join(root,'kucoinspot.ws-private.ts'),'KucoinSpotPrivateWebsocket',['refreshOrdersSoon','refreshBalanceSoon'])};
const cases=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(cases.length,42,'independent refresh floor');
const tag=h=>h===undefined?'none':typeof h==='number'?'number:'+h:'object:'+h.value;
(async()=>{const output=[];
for(const {kind,scenario:s} of cases){
 const trace=[],error=new TypeError('refresh failure'),oldObject={value:13},newObject={value:9},replaced={value:17},balance={value:55};
 const timerName=kind==='snapshot'?'snapshotRefreshTimer':kind==='orders'?'orderRefreshTimeout':'balanceRefreshTimeout';let queued,pending,resolve,reject,updated;
 const module={exports:{}};let receiver;
 const setTimeout=(cb,delay)=>{trace.push(`set:${delay}:${tag(receiver[timerName])}`);if(s==='set-error')throw error;if(s==='inline')pending=cb();else queued=cb;return newObject;};
 const clearTimeout=h=>{trace.push('clear:'+tag(h));if(typeof h==='object')assert.equal(h,oldObject,'source original cleared timer identity');if(s==='clear-error')throw error;};
 const code=ts.transpileModule('export class Selected {'+methods[kind==='snapshot'?'snapshot':'spot']+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 vm.runInNewContext('(function(module,exports){'+code+'\n})',{setTimeout,clearTimeout})(module,module.exports);receiver=new module.exports.Selected();
 receiver[timerName]=s==='zero'?0:['number','clear-error'].includes(s)?7:s==='object'?oldObject:undefined;receiver.isDisposed=s==='disposed';
 const fetch=()=>{trace.push('fetch:'+tag(receiver[timerName]));if(s==='reentrant')receiver[timerName]=replaced;
  if(kind==='snapshot'){if(s==='fetch-error')throw error;return Promise.resolve();}
  return new Promise((yes,no)=>{resolve=yes;reject=no;});};
 receiver.parent={fetchAndSetPrivateSnapshot:fetch,fetchAndSetOrders:fetch,fetchBalance:fetch};
 receiver.store={loaded:{markets:true,orders:false,balance:false},update(patch){trace.push('update');assert.equal(patch.balance,balance,'source decoded balance identity');updated={balance:patch.balance.value,markets:patch.loaded.markets,orders:patch.loaded.orders,loadedBalance:patch.loaded.balance};}};
 let schedule={ok:true},callback=null;
 try{receiver[kind==='snapshot'?'refreshSnapshotSoon':kind==='orders'?'refreshOrdersSoon':'refreshBalanceSoon']();}catch(e){assert.equal(e,error);schedule={error:true};}
 const fired=!!queued||s==='inline'&&schedule.ok;
 if(fired){if(s==='dispose-fire')receiver.isDisposed=true;if(s==='superseded')receiver[timerName]=replaced;
  if(queued){try{pending=queued();}catch(e){assert.equal(e,error);callback={error:true};}}
  if(s==='dispose-await')receiver.isDisposed=true;if(s==='loaded-mutate')receiver.store.loaded.orders=true;
  if(resolve){if(s==='fetch-error')reject(error);else resolve(kind==='balance'?balance:undefined);}
  if(!callback){try{await pending;callback={ok:true};}catch(e){assert.equal(e,error);callback={error:true};}}
 }
 output.push({trace,timer:tag(receiver[timerName]),schedule,callback,update:updated??null});
}
process.stdout.write(JSON.stringify(output));})().catch(e=>{console.error(e);process.exitCode=1;});
