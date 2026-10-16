// Original TS bodies; injected current child methods and ECMAScript intrinsics.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'isolated TypeScript compiler required');const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/polymarket');
const apiNames=['postOrder','postOrders','getOpenOrders','getTrades','getGammaMarkets','getPositions'],exchangeNames=['getAccount','validateAccount'],names=[...apiNames,...exchangeNames];
function bodies(file,klass,names){const source=fs.readFileSync(path.join(root,file),'utf8'),ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);const node=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);assert(node,klass);return names.map(name=>{const members=node.members.filter(m=>m.name?.getText(ast)===name);assert.equal(members.length,1,'unique original '+name);return source.slice(members[0].getFullStart(),members[0].end);}).join('\n');}
const constants=fs.readFileSync(path.join(root,'polymarket.constants.ts'),'utf8'),ast=ts.createSourceFile('constants.ts',constants,ts.ScriptTarget.Latest,true),endpoint=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='ENDPOINTS'));assert.equal(endpoint.length,1);
const cursorSource=fs.readFileSync(path.join(root,'polymarket.api.ts'),'utf8'),cursorAst=ts.createSourceFile('api.ts',cursorSource,ts.ScriptTarget.Latest,true),cursorNames=['INITIAL_CURSOR','END_CURSOR'];const cursors=cursorNames.map(name=>{const nodes=cursorAst.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(cursorAst)===name));assert.equal(nodes.length,1);return nodes[0].getText(cursorAst);}).join('\n');
const severityFile=fs.readFileSync(path.resolve(root,'../../../../logging/src/LogSeverity.ts'),'utf8'),severityAst=ts.createSourceFile('severity.ts',severityFile,ts.ScriptTarget.Latest,true),severity=severityAst.statements.filter(n=>ts.isEnumDeclaration(n)&&n.name.text==='LogSeverity');assert.equal(severity.length,1,'actual LogSeverity declaration');
let serializer;const context={exports:{},orderToJsonV2:(...args)=>serializer(...args)};
vm.runInNewContext(ts.transpileModule(`${severity[0].getText(severityAst)}
${endpoint[0].getText(ast)}\n${cursors}\nclass API{${bodies('polymarket.api.ts','PolymarketAPI',apiNames)}}\nclass Exchange{${bodies('polymarket.exchange.ts','PolymarketExchange',exchangeNames)}}\nexports.API=API;exports.Exchange=Exchange;exports.prototype=Object.prototype;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,context);
const ids=new WeakMap();function object(id,fields){const v=Object.fromEntries(fields);if(id!==null)ids.set(v,id);return v;}function array(id,items){const v=items.slice();if(id!==null)ids.set(v,id);return v;}
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function encode(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};if(typeof v==='boolean')return{kind:'bool',value:v};if(typeof v==='number')return{kind:'number',bits:bits(v)};if(typeof v==='string')return{kind:'text',units:units(v)};if(Array.isArray(v))return{kind:'array',id:ids.get(v)??null,items:Array.from({length:v.length},(_,i)=>i in v?encode(v[i]):{kind:'hole'})};return{kind:'object',id:ids.get(v)??null,fields:Object.entries(v).map(([k,v])=>[k,encode(v)])};}
const texts=['market/one','','🦀\ud800','\udfff','\0','x/y?z','#','__proto__','0','constructor',' final ','tail'];const limits=[undefined,0,-0,1,1.5,-1,NaN,Infinity,-Infinity,2,3,0.5];
function row(id){return object(id,[['marker',id],['text',texts[id%12]],['number',-0]]);}function rows(id,n,start){return array(id,Array.from({length:n},(_,i)=>row(start+i)));}
function fixture(name,i){const credentials=i===1?null:i===2?false:object(10,[['apiKey','owner-'+texts[i]],['secret','secret']]);const order=row(71),input=array(12,[object(13,[['order',order],['orderType','GTC']]),object(14,[['order',row(73)],['orderType','FAK']]),object(15,[['order',order],['orderType','GTD']])]);if(i===3)delete input[1];if(i===4)input.length=0;
const market=i===0?undefined:texts[i];let params;if(i===0)params=undefined;else if(i===1)params={limit:undefined,offset:null,active:false,closed:true};else params={active:i%2===0,closed:false,archived:undefined,limit:i===2?1.5:i===3?Infinity:i===4?0:i===5?-1:i===6?NaN:i===7?-Infinity:2,offset:i===8?-0:i===9?-2:3};
let replies=[],expectCount=null,direct=false,errorAt=null;
if(name==='getOpenOrders'||name==='getTrades'){
 if(i===0||i===3){replies=[rows(200,3,300)];expectCount=3;direct=true;}
 else if(i===10){replies=[object(200,[['data',rows(201,2,300)],['next_cursor','cursor/2']]),rows(202,1,400)];expectCount=1;direct=true;}
 else if(i===1||i===2){replies=[object(200,[['data',i===1?undefined:false],['next_cursor',i===1?'':null]])];expectCount=0;}
 else if(i===11){replies=[object(200,[['data',rows(201,3,300)],['next_cursor','cursor/2']]),object(202,[['data',rows(203,1,400)],['next_cursor','LTE=']])];expectCount=name==='getTrades'?0:4;}
 else{replies=[object(200,[['data',rows(201,2,300)],['next_cursor','cursor/2']]),object(202,[['data',rows(203,2,400)],['next_cursor','cursor/3']]),object(204,[['data',rows(205,1,500)],['next_cursor','LTE=']])];expectCount=5;
 if(name==='getTrades'){if(i===4)expectCount=1;if(i===5)expectCount=1;if(i===8)expectCount=0;if(i===9)expectCount=2;}}
 if(i===7)errorAt=1;
}else if(name==='getGammaMarkets'){
 if(i===4||i===5||i===6||i===7){replies=[rows(200,0,300),rows(201,0,400)];errorAt=1;}
 else if(i===0||i===1){replies=[rows(200,500,300),rows(201,1,900)];expectCount=501;}
 else if(i===2){replies=[rows(200,2,300),rows(201,1,400)];expectCount=3;}
 else if(i===3){replies=[rows(200,3,300)];expectCount=3;}
 else{replies=[rows(200,2,300),rows(201,2,400),rows(202,1,500)];expectCount=5;}
}else if(name==='getPositions'){
 replies=i===1?[rows(200,0,300)]:i===2?[rows(200,1,300)]:[rows(200,500,300),rows(201,500,900),rows(202,1,1500)];expectCount=i===1?0:i===2?1:1001;if(i===7)errorAt=1;
}else if(name==='postOrder'||name==='postOrders'){replies=[row(200)];if(i===8)errorAt=0;}
let wallet=i===1||i===2?null:row(20),exchangeCredentials=i===0||i===3||i===7||i===8||i===9||i===10||i===11?object(21,[['apiKey','exchange-owner']]):null;
return {credentials,order,input,market,limit:limits[i],params,replies,expectCount,direct,errorAt,wallet,exchangeCredentials,address:texts[i],signer:[0,-0,1,2,3,NaN,Infinity,-1,4,2,1,3][i],kind:['GTC','FOK','GTD','FAK'][i%4]};}
function args(name,f){switch(name){case'postOrder':return[f.order,f.kind];case'postOrders':return[f.input];case'getOpenOrders':return[f.market];case'getTrades':return[f.market,f.limit];case'getGammaMarkets':return[f.params];case'getPositions':return[f.address];default:return[];}}
(async()=>{const cases=[];
for(const name of names)for(let i=0;i<12;i++){
 const f=fixture(name,i),fixtureWire=Object.fromEntries(Object.entries(f).filter(([k])=>!['expectCount','direct','errorAt','kind'].includes(k)).map(([k,v])=>[k,k==='replies'?v.map(encode):encode(v)]));fixtureWire.kind=encode(f.kind);
 const instance=new(apiNames.includes(name)?context.exports.API:context.exports.Exchange)(),calls=[],logs=[],serializerCalls=[],assignments=[];let completed=0,pendingFinishes=[],derivations=0,balanceCalls=0;
 const failure={marker:47,message:i===9?'':i===10?0:i===11&&name==='validateAccount'?object(60,[['marker',60]]):i===11?'\ud800 failure':'original failure'};
 function pending(result,fails){return new Promise((resolve,reject)=>pendingFinishes.push(()=>{completed++;fails?reject(failure):resolve(result);}));}
 instance.getCredentials=()=>f.credentials;instance.wallet=f.wallet;instance.credentials=f.exchangeCredentials;instance.signerType=f.signer;
 instance.log=(text,severity)=>{logs.push([units(text),severity]);if(i===10&&name==='validateAccount')throw failure;};
 if(i===11&&name==='validateAccount')Object.defineProperty(failure.message,Symbol.toPrimitive,{value:()=>{instance.log=(text,severity)=>logs.push([units(text),'changed']);return '[object Object]';}});
 instance.wallet=f.wallet&&{getAddress:()=>{calls.push(['wallet','getAddress',[]]);return pending(f.address,i===8);}};
 instance.deriveApiCredentials=()=>{calls.push(['exchange','deriveApiCredentials',[]]);derivations++;return pending(undefined,i===6).then(v=>{if(i!==5)instance.credentials=object(22,[['apiKey','derived']]);return v;});};
 instance.api={getBalanceAllowance:signer=>{calls.push(['api','getBalanceAllowance',[encode(signer)]]);balanceCalls++;return pending(row(210),i>=7);}};
 for(const client of ['xhr','gammaXhr','dataXhr']){instance[client]={};for(const method of ['get','post'])instance[client][method]=(...args)=>{const at=calls.length;calls.push([client,method,args.map(encode)]);assert(at<f.replies.length,'finite source fixture ceiling');return pending({data:f.replies[at]},f.errorAt===at);};}
 serializer=(order,owner,kind)=>{serializerCalls.push([encode(order),encode(owner),encode(kind)]);assert.strictEqual(order,f.order,'source serializer original order');return object(90,[['serialized',order],['owner',owner],['orderType',kind]]);};
 // Live credentials and live Array.map holes, read after method/callable capture.
 if(name==='postOrders'&&(i===7||i===9)){let read=0;Object.defineProperty(f.credentials,'apiKey',{get(){read++;if(i===9&&read===1){delete f.input[1];f.input.push(object(16,[['order',row(75)],['orderType','FOK']]));}return 'owner-live-'+read;},enumerable:true});}
 if(i===11&&(name==='getOpenOrders'||name==='getTrades'))for(const key of ['market','limit'])Object.defineProperty(context.exports.prototype,key,{configurable:true,set(v){assignments.push([key,encode(v)]);}});
 let settled=false;const promise=instance[name](...args(name,f));promise.then(()=>settled=true,()=>settled=true);
 assert.equal(calls.length,apiNames.includes(name)?((name.startsWith('post')&&(i===1||i===2))?0:1):name==='getAccount'?(f.wallet?1:0):i===1||i===2?0:1,'source eager child count');
 let iterations=0;while(!settled){assert(iterations++<80,'source microtask/fixture ceiling');const finish=pendingFinishes.shift();if(finish)finish();await Promise.resolve();}
 let out,error;try{out=await promise;}catch(e){error=e;}
 const rejected=!!error;if(error){if(name.startsWith('post')&&(i===1||i===2))assert.equal(error.message,'Credentials not initialized');else assert.strictEqual(error,failure,'source original rejection identity');}
 else if(['getOpenOrders','getTrades','getGammaMarkets','getPositions'].includes(name)){assert(Array.isArray(out),'source result array');assert.equal(out.length,f.expectCount,'independent pagination result count');if(f.direct)assert.strictEqual(out,f.replies.at(calls.length-1),'source direct-array identity');else assert(!f.replies.includes(out),'source accumulated array fresh');if(out.length){const first=out[0];first.fromResult=17;assert(f.replies.some(reply=>(Array.isArray(reply)?reply:reply.data)?.includes(first)),'source original element identity');delete first.fromResult;}}
 else if(name.startsWith('post'))assert.strictEqual(out,f.replies[0],'source submitted response identity');
 else if(name==='getAccount'){assert.equal(out.userId,f.wallet?f.address:'unknown');assert.equal(out.affiliateId,null);}
 else{assert.equal(out.error,i===1||i===2||i===5?'No wallet configured':i===6?'original failure':i>=7?(failure.message||'Failed to validate account'):'','source validation result');}
 for(const key of ['market','limit'])delete context.exports.prototype[key];
 cases.push({assignments,name,i,fixture:fixtureWire,expectCount:f.expectCount,direct:f.direct,errorAt:f.errorAt,result:error?{error:{marker:error.marker??null,message:encode(error.message)}}:{value:encode(out)},calls,logs,serializerCalls,completed,derivations,balanceCalls});
}
assert.equal(cases.length,96,'independent eight-method twelve-fixture floor');assert.equal(cases[95].name,'validateAccount');assert.equal(cases[95].i,11);
// Whole-method lifetime: abandoning the returned promise still issues page two.
let first,second,started=0;const detached=new context.exports.API();detached.xhr={get(){started++;return new Promise(resolve=>{if(started===1)first=()=>resolve({data:{data:[row(300)],next_cursor:'next'}});else second=()=>resolve({data:{data:[row(400)],next_cursor:'LTE='}});});}};detached.getOpenOrders('ignored');first();for(let i=0;i<8;i++)await Promise.resolve();assert.equal(started,2,'source dropped observer still requests second page');second();
// Await-ready checkpoint: continuation must not issue the next page synchronously.
let readyCalls=0;const ready=new context.exports.API();ready.xhr={get(){readyCalls++;return Promise.resolve({data:{data:[],next_cursor:readyCalls===1?'next':'LTE='}});}};const readyPromise=ready.getOpenOrders();assert.equal(readyCalls,1,'source ready await yields');await readyPromise;assert.equal(readyCalls,2);
let resolveThenable,thenableReads=0,returnSettled=false;const assimilated=new context.exports.API();assimilated.getCredentials=()=>({apiKey:'owner'});serializer=(order)=>order;const final=row(999);const thenable={then(resolve){thenableReads++;resolveThenable=()=>resolve(final);}};assimilated.xhr={post(){return Promise.resolve({data:thenable});}};const returnPromise=assimilated.postOrder(row(71),'GTC');returnPromise.then(()=>returnSettled=true);for(let i=0;i<8;i++)await Promise.resolve();assert.equal(thenableReads,1,'source return thenable assimilated once');assert.equal(returnSettled,false,'source return thenable stays pending');resolveThenable();assert.strictEqual(await returnPromise,final,'source return resolution final identity');
const immediate=new context.exports.Exchange();immediate.wallet=null;const immediateTurns=[];immediate.getAccount().then(()=>immediateTurns.push('return'));queueMicrotask(()=>immediateTurns.push('marker'));await Promise.resolve();assert.deepEqual(immediateTurns,['return','marker'],'source non-thenable return has no await turn');
process.stdout.write(JSON.stringify({cases,detachedStarted:started,readyCalls,thenableReads,immediateTurns}));
})().catch(e=>{console.error(e);process.exitCode=1;});
