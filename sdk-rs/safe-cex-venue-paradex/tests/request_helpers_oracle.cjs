const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const text=fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts'),'utf8');
const source=ts.createSourceFile('exchange.ts',text,99,true);
const declarations=source.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]);
const names=['describeError','signerHeaders'];
const body=names.map(name=>{const ds=declarations.filter(d=>d.name.getText(source)===name);assert.equal(ds.length,1,'helper_source_identity '+name);return `export const ${ds[0].getText(source)};`;}).join('\n');
const moduleObject={exports:{}};let trace=[];
vm.runInNewContext(ts.transpileModule(body,{compilerOptions:{module:1,target:9}}).outputText,{module:moduleObject,exports:moduleObject.exports,JSON:{stringify(v){trace.push('json');return JSON.stringify(v);}},String(v){trace.push('string');return String(v);}});
const {describeError,signerHeaders}=moduleObject.exports;
const capture=v=>v===undefined?{type:'undefined'}:{type:'string',units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};
const cases=[];
function run(label,kind,input,value){trace=[];let outcome;try{outcome={ok:true,value:capture(describeError(value))};}catch(e){outcome={ok:false,identity:e===replacement?'replacement':'other'};}cases.push({label,kind,input,trace:[...trace],outcome});}
const replacement={name:'replacement'};
for(const input of ['null','false','true','0','-0','123.25','""','"text"','"é😀"','[]','[1,"a",null]','{}','{"b":2,"a":1}','{"2":"two","1":"one","x":"end"}'])run(input,'json',input,JSON.parse(input));
run('undefined','undefined',null,undefined);
run('function','undefined',null,function f(){});
run('symbol','undefined',null,Symbol('opaque'));
for(const n of ['0','123456789012345678901234567890','-7'])run('bigint-'+n,'fallback',n,BigInt(n));
const cycle={};cycle.self=cycle;run('cycle','fallback','[object Object]',cycle);
const throwing={toJSON(){throw replacement;},toString(){return 'fallback-distinct';}};run('toJSON-throw','fallback','fallback-distinct',throwing);
run('both-throw','replacement',null,{toJSON(){throw new Error('json failed');},toString(){throw replacement;}});
run('toJSON-undefined','undefined',null,{toJSON(){return undefined;},toString(){throw new Error('must not run');}});
run('fallback-surrogate','utf16',[0xd800],{toJSON(){throw replacement;},toString(){return String.fromCharCode(0xd800);}});
const headers=[];
for(const input of [null,{}, {'X-Signature':'sig-17','X-Timestamp':'time-29'}, {'constructor':'own','__proto__':'ignored literal'}, {'é😀':'distinct'}]){
 const value=input===null?undefined:input;const out=signerHeaders(value);
 const before=out.map(c=>({keys:Object.keys(c),headers:Object.entries(c.headers)}));
 const same=out.length===0?null:out[0].headers===value;
 if(value)value['late']='mutation-visible';
 headers.push({input:input===null?null:Object.entries(input).filter(([k])=>k!=='late'),length:out.length,before,same,late:out.length?out[0].headers.late:null});
}
assert.equal(cases.length,25,'source_case_count');assert.equal(headers.length,5,'source_header_count');
process.stdout.write(JSON.stringify({header:{arch:process.arch,node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),v8:process.versions.v8},cases,headers}));
