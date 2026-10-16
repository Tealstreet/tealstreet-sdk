// Execute the actual mapPositions initializer and imported source helpers.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'compiler_required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
const external=require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH));
const context=vm.createContext({structuredClone,process,setTimeout,clearTimeout,console});
const cache=new Map();
function execute(file) {
  if(cache.has(file))return cache.get(file);
  const exports={};cache.set(file,exports);
  const output=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
  vm.runInContext(`(function(exports,require){${output}\n})`,context,{filename:file})(exports,spec=>{
    if(spec.startsWith('@tealstreet/'))return execute(path.join(root,'packages',spec.slice('@tealstreet/'.length)+'.ts'));
    if(!spec.startsWith('.'))return external(spec);
    const target=path.resolve(path.dirname(file),spec);
    return execute(fs.existsSync(target+'.ts')?target+'.ts':path.join(target,'index.ts'));
  });return exports;
}
const {DefaultStore}=execute(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BlofinExchange');
const member=klass.members.find(m=>m.name?.text==='mapPositions');
assert(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),'positions_source_shape');
const selected=new Map();
for(const statement of ast.statements)if(ts.isVariableStatement(statement))for(const d of statement.declarationList.declarations)if(['BLOFIN_SUPPORTED_INST_TYPE','getNormalizedInstType','isKnownNonSwapInstrument'].includes(d.name.text))selected.set(d.name.text,d.initializer);
assert.equal(selected.size,3,'positions_helper_census');
let helpers='';for(const name of ['BLOFIN_SUPPORTED_INST_TYPE','getNormalizedInstType','isKnownNonSwapInstrument'])helpers+=`const ${name} = ${selected.get(name).getText(ast)};\n`;
Object.assign(context,execute(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.types.ts')));
Object.assign(context,execute(path.join(root,'packages/safe-cex/src/types.ts')));
Object.assign(context,execute(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')));
Object.assign(context,execute(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts')));
const factory=vm.runInContext(ts.transpileModule(`(function(){${helpers}return ${member.initializer.getText(ast)};})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
function observe(value){
 if(value===undefined)return{undefined:true};
 if(typeof value==='function')return{function:value.name};
 if(value===Object.prototype)return{prototype:true};
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return{number:Number.isNaN(value)?'nan':b.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));return value;
}
const market={id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,contractSize:0.125,info:{contractValue:'0.375'},precision:{amount:0.01,price:0.25},limits:{amount:{min:1,max:999},leverage:{min:2,max:17}}};
const base={instId:'BTC-USDT',instType:'SWAP',positionSide:'long',positions:'7.75',markPrice:'19.875',averagePrice:'18.125',leverage:'3.5',liquidationPrice:'4.25',unrealizedPnl:'5.375',marginMode:'isolated'};
const existing={id:'BTCUSDT:long',symbol:'BTCUSDT',side:'long',entryPrice:1.125,notional:2.25,leverage:13.5,unrealizedPnl:4.125,contracts:6.375,liquidationPrice:7.875,marginType:'isolated',positionMode:'hedged'};
const fetched={BTCUSDT:{buyLeverage:17.25,sellLeverage:23.75,isFetching:false}};
const cases=[];
function run(raw,state={},hostThrows=false,hostMutates=false){
 const initial={markets:[market],positions:[],fetchedLeverage:{},...state};
 const store=new DefaultStore();store.update(initial);const logs=[];const sentinel=new Error('host:position-warning');
 const fn=factory.call({store,emitAndLogError(message){logs.push(message);if(hostMutates)store.update({markets:[{...market,contractSize:0.5}]});if(hostThrows)throw sentinel;}});
 let result,invalid=false,infoAliases=0;
 try{const input=JSON.parse(raw);const output=fn(input);invalid=output.some(p=>!Object.values(context.PositionSide).includes(p.side)||typeof p.leverage!=='number');for(const p of output){assert(input.includes(p.info),'source_positions_info_alias');infoAliases++;}result={ok:observe(output)};}
 catch(error){result={error:error.name,message:error.message,identity:error===sentinel};}
 cases.push({raw,initial,result,logs,hostThrows,hostMutates,invalid,infoAliases});
}
run('[]');run(JSON.stringify([base]));
const values=['null','true','false','0','-0','1e400','-1e-400','9007199254740993','[]','["2.75tail"]','{}','{"toString":null}',...['',' ','-0','2.5tail','Infinity','NaN','constructor','__proto__','long','short','net','SWAP','spot'].map(JSON.stringify)];
for(const key of Object.keys(base)) {const absent={...base};delete absent[key];run(JSON.stringify([absent]));for(const value of values){const pairs=Object.entries(base).map(([k,v])=>JSON.stringify(k)+':'+(k===key?value:JSON.stringify(v)));run('[{'+pairs.join(',')+'}]');}}
for(const positionSide of ['long','short','net','unknown','constructor','__proto__'])for(const positions of ['-7.5','0','7.5','NaN','Infinity'])for(const leverage of ['0','NaN','4.5'])for(const mode of [0,1,2,3]){
 const id=positionSide==='net'?'BTCUSDT':`BTCUSDT:${positionSide==='short'||(positionSide!=='long'&&parseFloat(positions)<0)?'short':'long'}`;
 run(JSON.stringify([{...base,positionSide,positions,leverage,liquidationPrice:'NaN'}]),{positions:mode===1?[{...existing,id}]:mode===3?[{...existing,id,leverage:0,liquidationPrice:0}]:[],fetchedLeverage:mode===2?fetched:mode===3?{BTCUSDT:{...fetched.BTCUSDT,isFetching:true}}:{}});
}
const conflict=[{...base,positions:'3.5'},{...base,marginMode:'cross',positions:'4.25'}];
for(const hostThrows of [false,true])for(const hostMutates of [false,true])run(JSON.stringify(conflict),{},hostThrows,hostMutates);
run(JSON.stringify([{...base,instType:'SPOT',instId:'FIRST-SPOT'},...conflict]));
for(const key of ['marginMode','positionSide','positions','instId'])for(const a of [null,{},[],0,'0'])for(const b of [null,{},[],0,'0'])run(JSON.stringify([{...base,marginMode:'isolated',[key]:a},{...base,marginMode:'cross',positions:'9.5',[key]:b}]));
for(const contractSize of [0,-0,0.375,-2.5])for(const contractValue of ['0','NaN','Infinity','-0.125','2.75tail',{},null])run(JSON.stringify([base]),{markets:[{...market,contractSize,info:{contractValue}}]});
for(const info of [null,{},'text'])run(JSON.stringify([base]),{markets:[{...market,contractSize:0,info}]});
const noInfo={...market,contractSize:0};delete noInfo.info;run(JSON.stringify([base]),{markets:[noInfo]});
for(const key of Object.getOwnPropertyNames(Object.prototype)){
 run(JSON.stringify([{...base,instId:key}]));
 run(JSON.stringify([{...base,positions:'0',positionSide:key}]));
 run(JSON.stringify([{...base,leverage:'0'}]),{markets:[{...market,symbol:key}]});
}
for(const raw of ['null','{}','true','"text"','[null]',JSON.stringify([base,null]),JSON.stringify([{...base,instId:'missing'},base])])run(raw);
run('[{"instId":"BTC-USDT","positionSide":"net","positions":9007199254740993,"markPrice":1e400,"leverage":-0,"opaque":-1e-400}]');
for(const [a,b] of [['0','-0'],['1','1.0'],['9007199254740992','9007199254740993']]) {
 const left=JSON.stringify({...base,positions:'LEFT'}).replace('"LEFT"',a);
 const right=JSON.stringify({...base,marginMode:'cross',positions:'RIGHT'}).replace('"RIGHT"',b);
 run('['+left+','+right+']');
 assert.equal(cases.at(-1).logs.length,0,'source_positions_numeric_equality');
}
const boundaries=['[{"instId":"\\ud800"}]','['.repeat(140)+'0'+']'.repeat(140)];
for(const raw of boundaries){const store=new DefaultStore();assert.doesNotThrow(()=>factory.call({store,emitAndLogError(){}})(JSON.parse(raw)),'source_positions_boundary');}
process.stdout.write(JSON.stringify({header:{architecture:process.arch,nodeMajor:Number(process.versions.node.split('.')[0]),nodeVersion:process.version},cases,boundaries}));
