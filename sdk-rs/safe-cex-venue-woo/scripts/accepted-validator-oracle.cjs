// Independent expectations execute the maintained TypeScript validators.
'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=path.resolve(__dirname,'../../..'),src=path.join(root,'packages/safe-cex/src');
function load(file,names=[],imports={}) {
 const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(file,'utf8')+(names.length?'\nexport { '+names.join(',')+' };':''),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require(name){assert.ok(Object.hasOwn(imports,name),'reviewed source import '+name);return imports[name];}});return exports;
}
const spec={
 v3_public:['requireRecord','requireArray','requireString','optionalString','requireInteger','requireTimestamp','requireBoolean','requireDecimalString','requireInstrumentDecimal','optionalInstrumentDecimal','requireFiniteNumber'],
};
const apis={
 v3_public:load(path.join(src,'exchanges/woo/woo.v3.public.ts'),spec.v3_public,{'../../utils/orderbook':load(path.join(src,'utils/orderbook.ts'))}),
};
const bits=require('./number-bits.cjs');
function atom(value) {return typeof value==='number'?{kind:'number',bits:bits(value)}:typeof value==='string'?{kind:'string',text:value}:typeof value==='boolean'?{kind:'boolean',value}:value===null?{kind:'null'}:Array.isArray(value)?{kind:'array',length:value.length}:{kind:'object',length:Object.keys(value).length};}
const raws=[undefined,'null','true','false','0','-0','1','-1','2.5','9007199254740991','9007199254740992','1e400','-1e400','5e-324','1e-400','""','"alpha"','"beta"','" "','"\\ufeff2\\ufeff"','"\\u00852"','"0x10"','"0b11"','"0o17"','"1e3"','"-.0"','".5"','"1."','"1..2"','"Infinity"','"NaN"','"-0"','"0"','"00.0"','"BUY"','"SELL"','[]','["a",-0,true]','["b",2.5,false,null]','[1,2]','[null]','["0x10"]','{}','{"alpha":"first","zero":-0,"flag":true}','{"alpha":"second","zero":2.5,"flag":false,"extra":1}','{"toString":null}'];
raws.push(JSON.stringify('9'.repeat(310)));
const all={};
for(const [module,names] of Object.entries(spec)) for(const method of names) {
 const rust=method.replace(/[A-Z]/g,c=>'_'+c.toLowerCase()),name='safe_cex_venue_woo::'+module+'::'+rust;
 const corpus=[];
 let inputs=raws;
 const options=method==='requireFiniteNumber'?['0','1','-2','NaN','-Infinity','Infinity']:[null];
 for(const raw of inputs)for(const option of options) {
  const input={};if(raw!==undefined)input.raw=raw;if(option!==null)input.option=option;
  const value=raw===undefined?undefined:JSON.parse(raw);let output;
  try {
   let args=[value,'probe'];
   if(option!==null)args.push(Number(option));
   const got=apis[module][method](...args);
   let result=got===undefined?{present:false}:typeof got==='number'?{bits:bits(got)}:typeof got==='string'||typeof got==='boolean'?{value:got}:method==='requireRecord'?{length:Object.keys(got).length,alpha:got.alpha===undefined?null:atom(got.alpha),zero:got.zero===undefined?null:atom(got.zero),flag:got.flag===undefined?null:atom(got.flag)}:{values:Array.from(got,atom)};
   if(method.startsWith('optional')&&got!==undefined)result={present:true,...(method==='optionalInstrumentDecimal'?{value:atom(got)}:{value:got})};
   if(method==='requireInstrumentDecimal')result={value:atom(got)};
   if(module==='v3_public'&&(method==='requireRecord'||method==='requireArray'||method==='requireInstrumentDecimal'||(method==='optionalInstrumentDecimal'&&got!==undefined)))result.same=got===value;
   output={ok:true,...result};
  }catch(e){output={ok:false,error:e.message};}
  corpus.push({input,output});
 }
 all[name]=corpus;
}
for(const [module,method] of [['v3_public','required_stats_string'],['types','deserialize_present']]) {
 all['safe_cex_venue_woo::'+module+'::'+method]=['alpha','beta','',null,false,1,[],{}].map(input=>({input,output:typeof input==='string'?{ok:true,present:true,value:input}:{ok:false}}));
}
assert.equal(Object.keys(all).length,13);
if(process.argv.includes('--check')) {
 const contracts=JSON.parse(fs.readFileSync(path.join(root,'sdk-rs/safe-cex-parity/accepted-set/contracts/safe-cex-venue-woo.json')));
 for(const [name,cases]of Object.entries(all))assert.deepEqual(contracts[name]?.cases,cases,name+': reviewed live source expectations changed');
 console.log('Woo validator live source:13 exact reviewed corpora');
} else process.stdout.write(JSON.stringify(all,null,2)+'\n');
