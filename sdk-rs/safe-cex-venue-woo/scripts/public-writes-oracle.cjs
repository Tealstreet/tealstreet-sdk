// Actual venue methods plus the complete production Store constructor/mutations.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtime=require('./oracle-runtime.cjs');
const numberBits=require('./number-bits.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src');
function source(file){return fs.readFileSync(path.join(root,file),'utf8')}
function withoutImports(file){const text=source(file),ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);return ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n')}
const file='exchanges/woo/woo.ws-public.ts',text=source(file),ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
const cls=ast.statements.find(ts.isClassDeclaration);
const methods=['handleTickersStreamEvents','handleBBOStreamEvents','handleMarkPricesStreamEvents'];
const members=methods.map(name=>{const member=cls.members.find(n=>n.name?.getText(ast)===name);assert.ok(member,name);return member.getText(ast)}).join('\n');
const types=ts.createSourceFile('types.ts',source('types.ts'),ts.ScriptTarget.Latest,true);
const enums=types.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(types)).join('\n');
const program=enums+'\n'+withoutImports('utils/clone.ts')+'\n'+withoutImports('store/store.base.ts')+'\n'+withoutImports('exchanges/woo/woo.utils.ts')+'\nclass Witness {\n'+members+'\n}\nglobalThis.Store=DefaultStore;globalThis.Witness=Witness;';
const code=ts.transpileModule(program,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const cases=[];
const symbols=['PERP_BTC_USDT','PERP_ETH_USDT','SPOT_BTC_USDT','perp_BTC_USDT','PERP_MISSING_USDT','PERP_constructor_','PERP___proto__','PERP_toString_'];
for(const method of methods)for(const symbol of symbols)for(const throws of [false,true]){
 cases.push({method,throws,rows:[{symbol,close:11,amount:12,volume:13,bid:14,ask:15,price:16},{symbol:'PERP_ETH_USDT',close:21,amount:22,volume:23,bid:24,ask:25,price:26}]});
}
for(const method of methods)for(const value of ['-0','-1e-400','1e400','-1e400','null','"3"','false','{}'])cases.push({method,throws:false,raw:'[{"symbol":"PERP_BTC_USDT","close":'+value+',"amount":'+value+',"volume":'+value+',"bid":'+value+',"ask":'+value+',"price":'+value+'}]'});
for(const method of methods)for(const rows of [[{symbol:'PERP_BTC_USDT'}],[{symbol:null}],[{}],[],[{symbol:'PERP_BTC_USDT',close:1},{symbol:null},{symbol:'PERP_ETH_USDT',close:3}]])cases.push({method,throws:false,rows});
const results=cases.map(input=>{
 const context=vm.createContext({exports:{},memoize:runtime('lodash/memoize'),structuredClone,numberBits});
 vm.runInContext(code,context);
 context.input=input;
 vm.runInContext(`
 const store=new Store();
 const ticker=symbol=>({id:symbol,symbol,bid:1,ask:2,last:3,mark:4,index:5,percentage:6,openInterest:7,fundingRate:8,fundingTime:9,volume:10,quoteVolume:20});
 store.update({tickers:[ticker('BTCUSDT'),ticker('ETHUSDT')]});
 const notifications=[];
 store.subscribe(()=>{notifications.push('first');if(input.throws)throw new Error('subscriber failure')});
 store.subscribe(()=>notifications.push('second'));
 const worker=new Witness();worker.parent={store};worker.store=store;
 let error=null;try{worker[input.method](input.raw?JSON.parse(input.raw):input.rows)}catch(e){error={name:e.name,message:e.message}}
 function pack(value){if(value===undefined)return {$type:'undefined'};if(typeof value==='number'){return {$number:numberBits(value)}}if(Array.isArray(value))return value.map(pack);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,pack(v)]));return value}
 globalThis.result={input,notifications,error,tickers:pack(store.tickers)};
 `,context);
 return JSON.parse(JSON.stringify(context.result));
});
assert.ok(results.some(r=>r.error?.message==='subscriber failure'&&r.notifications.join(',')==='first'),'subscriber throw observed after commit');
// Pin the mutation-before-notification boundary independently of serialization.
const committed=results.find(r=>r.input.method==='handleTickersStreamEvents'&&r.input.throws&&r.input.rows[0].symbol==='PERP_BTC_USDT');
assert.equal(committed.tickers[0].last.$number,'4026000000000000','first patch commits before subscriber throw');
assert.equal(committed.tickers[1].last.$number,'4008000000000000','subscriber throw aborts remaining batch');
const target=path.join(__dirname,'../tests/fixtures/public-writes.json'),bytes=JSON.stringify(results,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(fs.readFileSync(target,'utf8'),bytes,'WOO actual Store writer drift');else fs.writeFileSync(target,bytes);
console.log('WOO actual Store writer cases:',results.length);
