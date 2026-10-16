import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url);
const types=fs.readFileSync(new URL('packages/safe-cex/src/types.ts',root),'utf8');
const ast=ts.createSourceFile('types.ts',types,ts.ScriptTarget.Latest,true);
const enums={};
for(const name of ['OrderSide','OrderStatus','OrderType','PositionSide']) {
 const declaration=ast.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text===name);assert.ok(declaration,name);
 vm.runInNewContext(ts.transpileModule(declaration.getText(ast),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:enums});
}
const source=fs.readFileSync(new URL('packages/safe-cex/src/exchanges/binance/binance.order-history.ts',root),'utf8');
function load(corrected) {
 const exports={};
 const suffix=corrected?'\n[ORDER_TYPE,ORDER_STATUS,POSITION_SIDE].forEach(table=>Object.setPrototypeOf(table,null));':'';
 vm.runInNewContext(ts.transpileModule(source+suffix,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,require:id=>{assert.equal(id,'../../types');return enums;}});
 return exports.mapBinanceHistoricalOrder;
}
const original=load(false),corrected=load(true);
const norm=v=>v===undefined?{undefined:true}:typeof v==='function'?{function:v.name}:typeof v==='number'?{number:Object.is(v,-0)?'-0':String(v)}:v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,norm(x)])):v;
function evaluate(fn,row) {try{return {value:norm(fn(row))};}catch(e){return {error:e.name};}}
const base={orderId:'12',symbol:'BTCUSDT',time:1,side:'BUY',origType:'STOP_MARKET',type:'MARKET',status:'FILLED',price:'1',avgPrice:'2',origQty:'3',executedQty:'2',reduceOnly:false,positionSide:'LONG',updateTime:2};
const values=[undefined,null,false,true,0,-1,1,1e21,'',' ','0x10','12abc','-0','NaN','Infinity','1e999','-1e999',[],[1],[null],['12'],['12','3'],{},{toString:0},{toString:null},[{toString:0}],{valueOf:2},'BUY','buy','SELL','ſell','LIMIT','MARKET','STOP','STOP_MARKET','TAKE_PROFIT','TAKE_PROFIT_MARKET','TRAILING_STOP_MARKET','NEW','PARTIALLY_FILLED','FILLED','CANCELED','EXPIRED','REJECTED','BOTH','LONG','SHORT','constructor','__proto__','toString','valueOf','hasOwnProperty'];
const inheritedNames=Object.getOwnPropertyNames(Object.prototype);
assert.equal(inheritedNames.length,12,'complete Object.prototype own-name inventory');
for(const name of inheritedNames)if(!values.includes(name))values.push(name);
const fallbackValues=[...values,'\x0012tail'];
for(let i=0;i<256;i++) values.push(String.fromCharCode(i)+'12tail');
const rows=[];
for(const key of Object.keys(base)) for(const value of values) rows.push(JSON.stringify({...base,[key]:value}));
for(const origType of fallbackValues) for(const type of ['MARKET','LIMIT',{toString:null}]) rows.push(JSON.stringify({...base,origType,type}));
for(const key of Object.keys(base)) for(const literal of ['1e400','-1e400','-0']) rows.push('{'+Object.entries(base).map(([k,v])=>JSON.stringify(k)+':'+(k===key?literal:JSON.stringify(v))).join(',')+'}');
rows.push(JSON.stringify({...base,orderId:'',symbol:{toString:null}}),JSON.stringify({...base,time:0,side:{toString:null}}));
for(const origType of inheritedNames)for(const field of ['status','price','avgPrice','origQty','executedQty','positionSide','updateTime'])rows.push(JSON.stringify({...base,origType,[field]:{toString:null}}));
// Multiple inherited fields retain source field order in the declared refusal projection.
for(const name of inheritedNames)for(const [first,second] of [['origType','status'],['origType','positionSide'],['status','positionSide']])rows.push(JSON.stringify({...base,[first]:name,[second]:'toString'}));
const cases=rows.map(row=>{const parsed=JSON.parse(row);return {row,original:evaluate(original,parsed),expected:evaluate(corrected,parsed)};});
const differences=cases.filter(c=>JSON.stringify(c.original)!==JSON.stringify(c.expected));
assert.ok(differences.some(c=>c.original.value?.type?.function==='Object'),'pin inherited constructor bug');
assert.ok(differences.some(c=>c.original.value?.status?.function==='toString'),'pin inherited status bug');
assert.ok(differences.some(c=>c.original.value?.positionSide?.function==='valueOf'),'pin inherited position bug');
assert.ok(differences.some(c=>c.original.value && c.expected.error==='TypeError'),'pin inherited origType lazy fallback difference');
for(const name of inheritedNames){
 for(const [input,output] of [['origType','type'],['status','status'],['positionSide','positionSide']]){
  const witness=cases.find(c=>JSON.parse(c.row)[input]===name&&c.original.value);
  assert.ok(witness,`inherited ${name} ${input} witness exists`);
  if(name==='__proto__')assert.deepEqual(witness.original.value[output],{},`inherited ${name} ${output} object before JSON`);
  else assert.equal(witness.original.value[output].function,name==='constructor'?'Object':name,`inherited ${name} ${output} function before JSON`);
 }
}
for(const name of inheritedNames){
 const result=original({...base,origType:name});
 const serialized=JSON.stringify(result),wire=JSON.parse(serialized);
 assert.equal(wire.id,base.orderId,`inherited ${name} serialization retains the record`);
 if(name==='__proto__')assert.deepEqual(wire.type,{},'inherited __proto__ serializes as an object field');
 else assert.equal(Object.hasOwn(wire,'type'),false,`inherited ${name} serialization omits only the type field`);
 assert.notEqual(serialized,JSON.stringify(null),`inherited ${name} whole-map refusal does not converge`);
}
// Apply the declared closed-output refusal only AFTER the original method has
// finished: a later source coercion error must retain its precedence.
for(const c of cases){
 c.prototypeFree=c.expected;
 c.expected=c.original;
 if(!c.original.value)continue;
 for(const field of ['type','status','positionSide']){
  const value=c.original.value[field];
  if(value&&typeof value==='object'&&!value.undefined){
   const key=value.function?(value.function==='Object'?'constructor':value.function):'__proto__';
   c.expected={refusal:{field,key,kind:value.function?'function':'object'}};
   break;
  }
 }
}
process.stdout.write(JSON.stringify(cases));
