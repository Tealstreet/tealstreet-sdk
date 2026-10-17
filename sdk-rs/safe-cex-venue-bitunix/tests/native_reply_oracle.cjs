// Execute the unchanged maintained batch body and uuid utility; no frozen output corpus.
const {ts,fs,path,vm,root,assert,load}=require('./oracle-loader.cjs');
const lodash=require('lodash');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration);
const member=cls.members.filter(n=>n.name?.getText(ast)==='placeOrderBatch');
assert.equal(member.length,1);
const code=ts.transpileModule('(function(){return ('+member[0].initializer.getText(ast)+');})',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const uuidFile=path.join(root,'packages/safe-cex/src/utils/uuid.ts');
const uuidCode=ts.transpileModule(fs.readFileSync(uuidFile,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const uuidModule={exports:{}};
vm.runInThisContext('(function(require,module,exports){'+uuidCode+'\n})')((id)=>{assert.equal(id,'nanoid');return {nanoid:()=> 'fixture-_nonce'};},uuidModule,uuidModule.exports);
const uuid=uuidModule.exports.uuid;
const specimens=[
 {successList:[null,{orderId:'A'}],failureList:[]},
 {successList:[{orderId:'prefix'},null,{orderId:'A'}],failureList:[{errorMsg:'unreachable'}]},
 {successList:[{orderId:'A'}],failureList:[null,{errorMsg:'unreachable'}]},
 [{orderId:'prefix'},null,{orderId:'A'}],
 {successList:[{},0,false,''],failureList:[]},
 null,
];
(async()=>{
 const cases=[];
 for(const reply of specimens){
  const errors=[];let calls=0;
  const owner={unlimitedXHR:{post:async()=>{calls++;return {data:{code:0,data:reply}};}},emitAndLogError:m=>errors.push(m)};
  const method=vm.runInNewContext(code,{groupBy:lodash.groupBy,chunk:lodash.chunk,omit:lodash.omit,sleep:async()=>{},uuid,ENDPOINTS:load(path.join(root,'packages/safe-cex/src/exchanges/bitunix/bitunix.types.ts')).ENDPOINTS}).call(owner);
  const ids=await method(Array.from({length:7},()=>({symbol:'BTCUSDT',orderType:'LIMIT'})));
  cases.push({reply,ids,errors,calls});
 }
 console.log(JSON.stringify({uuid:uuid(),cases}));
})().catch(e=>{console.error(e);process.exit(1);});
