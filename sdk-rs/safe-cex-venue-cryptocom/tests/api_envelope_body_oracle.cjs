const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'ENVELOPE-BODY compiler path required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/cryptocom/cryptocom.api.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file,'utf8'), ts.ScriptTarget.Latest, true);
const units = text => Array.from({length:text.length}, (_,i) => text.charCodeAt(i));
function build(name, deps) {
 const declarations=source.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).filter(d=>d.name.getText(source)===name);
 assert.equal(declarations.length,1,'ENVELOPE-BODY actual declaration '+name);
 assert(ts.isArrowFunction(declarations[0].initializer));
 const compiled=ts.transpileModule('return ('+declarations[0].initializer.getText(source)+');',{reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022}});
 assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 return new Function(...Object.keys(deps),'"use strict";\n'+compiled.outputText)(...Object.values(deps));
}
function bits(n){const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');}
function classify(v, root) {
 if(typeof v==='string')return {kind:'string',units:units(v)};
 if(typeof v==='number')return {kind:'number',bits:bits(v)};
 if(typeof v==='object'&&v!==null)return {kind:'object',role:v===root?'root':'params'};
 return {kind:'other'};
}
const revoked=target=>{const p=Proxy.revocable(target,{});p.revoke();return p.proxy;};
const rows=[];
const method='private/create-order',key='k\ud800\udfff';
const valid=()=>({method,api_key:key,params:{},sig:'',nonce:0});
const envelopeSpecs=[
 ['undefined',undefined],['null',null],['primitive',7],['function',()=>{}],['array',[]],['revoked-root',revoked({})],['valid',valid()],
 ['method-missing',{}],['method-case',{...valid(),method:'PRIVATE/create-order'}],['method-object',{...valid(),method:{[Symbol.toPrimitive](){throw Error('strict compare coerced');}}}],
 ['key-missing',{...valid(),api_key:undefined}],['key-mismatch',{...valid(),api_key:'k'}],['key-boxed',{...valid(),api_key:new String(key)}],
 ['params-null',{...valid(),params:null}],['params-array',{...valid(),params:[]}],['params-function',{...valid(),params:()=>{}}],['params-boxed',{...valid(),params:new Number(1)}],['params-revoked',{...valid(),params:revoked({})}],
 ['sig-missing',{...valid(),sig:undefined}],['sig-boxed',{...valid(),sig:new String('sig')}],['sig-surrogate',{...valid(),sig:'\ud800'}],
 ['nonce-nan',{...valid(),nonce:NaN}],['nonce-infinity',{...valid(),nonce:Infinity}],['nonce-negative-zero',{...valid(),nonce:-0}],['nonce-string',{...valid(),nonce:'0'}],['nonce-bigint',{...valid(),nonce:0n}],['nonce-boxed',{...valid(),nonce:new Number(0)}],
 ...['method','api_key','params','sig','nonce'].map(field=>['getter-'+field,valid(),field]),
 ['mutating-getter',valid()],
];
assert.equal(envelopeSpecs.length,33,'ENVELOPE-BODY independent envelope floor');
for(const [id,raw,throwField] of envelopeSpecs){
 const events=[],reads=[];let thrown;let input=raw;
 if(id==='mutating-getter')input={...valid(),api_key:'wrong'};
 if(typeof input==='object'&&input!==null&&!['array','revoked-root'].includes(id)) {
  input=new Proxy(input,{get(target,field,receiver){
   events.push({kind:'get',field});
   if(field===throwField){thrown=Error('getter '+field);throw thrown;}
   if(id==='mutating-getter'&&field==='method')target.api_key=key;
   const value=Reflect.get(target,field,receiver);reads.push({field,value:classify(value,input)});return value;
  }});
 }
 const arrayResults=[];
 const record=build('isRecord',{Array:{isArray(value){
  const role=value===input?'root':'params';events.push({kind:'array',role});
  try{const result=Array.isArray(value);arrayResults.push({role,result});return result;}
  catch(error){thrown=error;arrayResults.push({role,error:true});throw error;}
 }}});
 const fn=build('isAuthenticatedEnvelope',{isRecord:record});
 let result;
 try{result={kind:'boolean',value:fn(input,method,key)};assert.equal(typeof result.value,'boolean');}
 catch(error){assert.strictEqual(error,thrown,'ENVELOPE-BODY original native error');result={kind:'original'};}
 rows.push({group:'envelope',id,input:classify(input,input),method:units(method),key:units(key),reads,arrayResults,throwField:throwField??null,events,result});
}
const cyclic={};cyclic.self=cyclic;
const getterThrow={get x(){throw Error('json getter');}};
const toJsonThrow={toJSON(){throw Error('toJSON');},toString(){return 'native fallback';}};
const bothThrow={toJSON(){throw Error('json');},[Symbol.toPrimitive](){throw Error('string');}};
const bodySpecs=[
 ['empty',''],['literal','{"a":1}'],['surrogate','\ud800X\udfff'],['undefined',undefined],['symbol',Symbol('x')],['function',()=>{}],['null',null],['false',false],['number',17.5],['negative-zero',-0],['nan',NaN],['infinity',Infinity],
 ['object',{a:1,b:undefined}],['array',[undefined,NaN,'\ud800']],['boxed',new String('str')],['date',new Date(0)],['bigint',40101n],['cyclic',cyclic],['getter-throw',getterThrow],['toJSON-throw',toJsonThrow],['both-throw',bothThrow],
 ['mutating-toJSON',{toJSON(){this.changed=true;throw Error('json');},toString(){return this.changed?'changed':'before';}}],
];
assert.equal(bodySpecs.length,22,'ENVELOPE-BODY independent stringifier floor');
for(const [id,input] of bodySpecs){
 const events=[];let jsonError=false,stringError=false,jsonResult=null,stringResult=null,thrown;
 const fn=build('stringifyResponseBody',{
  JSON:{stringify(value){assert(Object.is(value,input),'ENVELOPE-BODY JSON input identity');events.push({kind:'json'});try{const result=JSON.stringify(value);jsonResult=result===undefined?null:units(result);return result;}catch(error){jsonError=true;throw error;}}},
  String(value){assert(Object.is(value,input),'ENVELOPE-BODY String input identity');events.push({kind:'string'});try{const result=String(value);stringResult=units(result);return result;}catch(error){stringError=true;thrown=error;throw error;}},
 });
 let result;
 try{const value=fn(input);assert(value===undefined||typeof value==='string','ENVELOPE-BODY native string/undefined');result=value===undefined?{kind:'undefined'}:{kind:'string',units:units(value)};}
 catch(error){assert.strictEqual(error,thrown,'ENVELOPE-BODY retained String failure');result={kind:'original'};}
 rows.push({group:'body',id,primitive:typeof input==='string'?units(input):null,jsonResult,jsonError,stringResult,stringError,events,result});
}
assert.equal(rows.length,55,'ENVELOPE-BODY independent total floor');
process.stdout.write(JSON.stringify(rows));
