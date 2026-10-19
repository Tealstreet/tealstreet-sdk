const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/base.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
assert.equal(tree.parseDiagnostics.length, 0);
const base = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BaseExchange');
const names = ['fetchAndSetOrders', 'fetchAndSetPrivateSnapshot', 'getOrFetchPositionsForSymbol'];
const members = names.map(name => {
  const member = base.members.find(n => n.name?.getText(tree) === name && n.initializer);
  assert.ok(member, name);
  assert.ok(member.initializer.modifiers?.some(m => m.kind === ts.SyntaxKind.AsyncKeyword), name);
  return member.getText(tree);
});
const additional = ['accountMarketOverlay', 'adaptExternalMarkets', 'loadAccountMarketOverlay', 'callMethodInWorker'].map(name => {
  const member = base.members.find(n => n.name?.getText(tree) === name);
  assert.ok(member, name);
  return member.getText(tree);
});
const scope = { exchangeMetadata: {}, nanoid: () => 'request', setTimeout, clearTimeout };

vm.runInNewContext(ts.transpileModule(`class Source { ${members.concat(additional).join('\n')} } globalThis.Source=Source;`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022 },
}).outputText, scope);
(async () => {
  const rows = [];
  for (const method of names) {
    const receiver = new scope.Source();
    const cause = { witness: method };
    receiver.isPublicDataOnly = false;
    receiver.isDisposed = false;
    Object.defineProperty(receiver, 'fetchOrders', { get() { throw cause; } });
    Object.defineProperty(receiver, 'store', { get() { throw cause; } });
    const argument = { valueOf() { throw cause; } };
    let result;
    let syncThrow = false;
    try { result = receiver[method](argument); } catch { syncThrow = true; }
    const promise = result != null && typeof result.then === 'function';
    let caught = false;
    if (promise) await result.catch(error => { caught = error === cause; });
    rows.push({ method, syncThrow, promise, caught });
  }
  const guarded = [];
  for (const kind of ['public', 'disposed']) {
    const receiver = new scope.Source();
    receiver.isPublicDataOnly = kind === 'public';
    receiver.isDisposed = kind === 'disposed';
    let coerced = false;
    await receiver.fetchAndSetPrivateSnapshot({ valueOf() { coerced = true; throw Error('unreached'); } });
    guarded.push({ kind, coerced });
  }
  const helperFile = path.resolve(__dirname, '../../../packages/safe-cex/src/createApiWithSecondaryKey.ts');
  const helperTree = ts.createSourceFile(helperFile, fs.readFileSync(helperFile, 'utf8'), ts.ScriptTarget.Latest, true);
  assert.equal(helperTree.parseDiagnostics.length, 0);
  const declaration = helperTree.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(helperTree) === 'createApiWithSecondaryKey'));
  assert.ok(declaration);
  const helperScope = {getTransport: () => ({publicCorsProxy: () => undefined})};
  vm.runInNewContext(ts.transpileModule(declaration.getText(helperTree).replace(/^export /, '') + '\nglobalThis.helper=createApiWithSecondaryKey;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, helperScope);
  const secondaryOwners = [];
  for (const mode of ['single','split','throw']) {
    const cause = {units:'\ud800'};
    const payload = {opaque:undefined};
    const signer = () => undefined;
    const options = {key:'primary',secret:'secret',secondary_key:mode==='single'?'':'secondary',remoteSigner:signer,payloadSigner:signer,unread:'\ud800'};
    let calls=0;
    try {
      const [xhr,primary,secondary]=helperScope.helper(options,{},opts=>{
        calls++;
        if(mode==='throw')throw cause;
        assert.equal(opts.remoteSigner,signer);assert.equal(opts.payloadSigner,signer);assert.equal(opts.unread,'\ud800');
        return {tag:opts.key,get(value){return value;},post(value){return [this.tag,value];}};
      });
      const get=xhr.get(payload), post=xhr.post(payload);
      secondaryOwners.push({mode,calls,shared:xhr===primary&&primary===secondary,getIdentity:get===payload,postIdentity:post[1]===payload,postTag:post[0],units:Array.from(options.unread, c=>c.charCodeAt(0))});
    }catch(error){secondaryOwners.push({mode,calls,causeIdentity:error===cause});}
  }
  const overlayOwners=[];
  for(const mode of ['null','undefined','identity']) {
    const receiver=new scope.Source();receiver.store={markets:[]};receiver.isDisposed=false;
    let calls=0;
    const overlay=mode==='null'?null:mode==='undefined'?undefined:markets=>{calls++;return markets;};
    const result=await receiver.loadAccountMarketOverlay(()=>overlay);
    const markets=receiver.adaptExternalMarkets([]);
    overlayOwners.push({mode,undefinedResult:result===undefined,calls,length:markets.length});
  }
  const workerOwners=[];
  for(const success of [true,false]) {
    const receiver=new scope.Source();const listeners=new Map();let request;
    receiver.emitter={emit(event,value){request=value;},on(event,fn){listeners.set(event,fn);},off(event){listeners.delete(event);}};
    receiver.afterMethodCalledInWorker=()=>{};
    const promise=receiver.callMethodInWorker('removeOrders',[{id:'wire'}]);
    assert.deepEqual(JSON.parse(JSON.stringify(request.args)),[[{id:'wire'}]]);
    assert.equal(request.method,'removeOrders');
    listeners.get('callMethod:done')({requestId:request.requestId,success,result:success?{removed:'wire'}:{name:'WorkerFailure',message:'rejected',code:409,stack:'worker stack'}});
    try {const result=await promise;workerOwners.push({success:true,removed:result.removed});}
    catch(error){workerOwners.push({success:false,name:error.name,message:error.message,code:error.code,stack:error.stack});}
  }
  const receiver=new scope.Source();receiver.store={markets:[]};receiver.isDisposed=false;
  const cause={name:'RangeError',message:'b\ud800'};
  await receiver.loadAccountMarketOverlay(()=>()=>{throw cause;});
  let overlayThrow;
  try {receiver.adaptExternalMarkets([]);overlayThrow={propagates:false};}
  catch(error){overlayThrow={propagates:true,identity:error===cause,name:error.name,units:Array.from(error.message,c=>c.charCodeAt(0))};}
  process.stdout.write(JSON.stringify({ rows, guarded, secondaryOwners, overlayOwners, workerOwners, overlayThrow }));
})().catch(error => { console.error(error); process.exitCode = 1; });
