'use strict';
const fs=require('node:fs'), path=require('node:path'), ts=require('typescript');
const root=path.resolve(__dirname,'../../..');
function load(name){const file=path.join(root,'packages/ccxt-compat/src/functions',name+'.ts');const m={exports:{}};new Function('exports','require','module',ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText)(m.exports,require,m);return m.exports;}
const api={...load('ccxt_exchange'),...load('number'),...load('type')};
const special=v=>v&&v.special?({undefined:undefined,nan:NaN,infinity:Infinity,negativeInfinity:-Infinity,negativeZero:-0,bigint:123n,surrogate:'\ud800'})[v.special]:v;
const cases=[];
for(const x of [0,-0,1e-8,-1e-8,1.234e23,NaN,Infinity,-Infinity,undefined,null,true,123n,'1e-8','\ud800'])cases.push(['numberToString',[x]]);
for(const x of [undefined,null,true,0,1,3,-2,'2.9','0x10','garbage','\ud800'])cases.push(['parsePrecision',[x]]);
for(const x of ['12.3456','-12.3456','0.00098765','999.999','1.45','0','-0','1..2','1e-8','\ud800',undefined,null,NaN,Infinity,123n])for(const mode of [0,1])for(const count of [2,3])cases.push(['decimalToPrecision',[x,mode,2,count,5]]);
for(const args of [['123.45',1,-1],['123.45',0,-1],['1.2345',1,2.5],['1.2345','1',2],['1.2345',1,'2'],['1.2345',1,null],['1.2345',1,undefined],['1.2345',1,2,2,6],['1.2345',1,.05,4,5],['-1.2345',1,'0.05junk',4,5],['1',1,0,4,5],['1',1,-1,4,5]])cases.push(['decimalToPrecision',args]);
for(const x of [undefined,null,'',0,-0,NaN,Infinity,true,123n,'\ud800'])cases.push(['safeString',[{x},'x','default']]);
function encode(v){if(v===undefined)return{special:'undefined'};if(typeof v==='number'&&!Number.isFinite(v))return{special:Number.isNaN(v)?'nan':v>0?'infinity':'negativeInfinity'};if(Object.is(v,-0))return{special:'negativeZero'};if(typeof v==='bigint')return{special:'bigint'};if(v==='\ud800')return{special:'surrogate'};if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)]));return v;}
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const encoded=cases.map(([name,args])=>({name,args:args.map(encode),expected:(()=>{try{const v=api[name](...args);return v===undefined?{kind:'undefined'}:{kind:'return',value:typeof v==='string'?units(v):v};}catch(e){return{kind:'throw',name:e.name,message:units(e.message)};}})()}));
console.log(JSON.stringify(encoded));
