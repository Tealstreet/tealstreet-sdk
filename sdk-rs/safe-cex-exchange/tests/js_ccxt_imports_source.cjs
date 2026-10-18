'use strict';
const fs=require('node:fs'), path=require('node:path'), ts=require('typescript');
const root=path.resolve(__dirname,'../../..');
function load(name){const file=path.join(root,'packages/ccxt-compat/src/functions',name+'.ts');const m={exports:{}};new Function('exports','require','module',ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText)(m.exports,require,m);return m.exports;}
const api={...load('ccxt_exchange'),...load('number'),...load('type')};
const special=v=>v&&v.special?({undefined:undefined,nan:NaN,infinity:Infinity,negativeInfinity:-Infinity,negativeZero:-0,bigint:123n,surrogate:'\ud800'})[v.special]:v;
if(process.argv[2]==='effects'){
 const log=[], token={token:true};let reads=0;
 const value={get toString(){log.push('string:get');return function(){log.push('string:call:'+String(this===value));return '\ud800'};}};
 const key={toString(){log.push('key');return 'x'}};
 const o={get x(){log.push('field:'+ ++reads);return reads===3?value:1}};
 const result=api.safeString(o,key,token);
 let thrown=false;try{api.numberToString({get toString(){log.push('throw:get');throw token}})}catch(e){thrown=e===token;}
 const returned=api.numberToString({toString(){log.push('return:call');return token}})===token;
 const defaults=api.safeString({x:null},'x',token)===token;
 const f=function(){};Object.defineProperty(f,'x',{get(){throw Error('must not read')}});
 const functionDefault=api.safeString(f,'x',token)===token;
 const precision=api.parsePrecision({toString(){log.push('parse:string');return '3x'}});
 const digits={valueOf(){log.push('digits:number');return 2}};
 const x={toString(){log.push('decimal:string');return '1.2345'}};
 const decimal=api.decimalToPrecision(x,1,digits,2,5);
 const strictMode=api.decimalToPrecision('1.235',{valueOf(){throw Error('mode must not coerce')}},2,2,5);
 console.log(JSON.stringify({log,units:Array.from({length:result.length},(_,i)=>result.charCodeAt(i)),thrown,returned,defaults,functionDefault,precision,decimal,strictMode}));process.exit(0);
}
if(process.argv[2]==='precision'){
 const cases=['','1','0.0100','1.2500','1e-8','1E+5','-2.300e-7','abc','abc1e-2','1e','0.00000001','123.456789'];
 console.log(JSON.stringify(cases.map(input=>{const value=api.precisionFromString(input);const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return {input,nan:Number.isNaN(value),bits:bytes.toString('hex'),json:JSON.stringify(value)};})));process.exit(0);
}
const cases=[];
for(const x of [0,-0,1e-8,-1e-8,1.234e23,NaN,Infinity,-Infinity,undefined,null,true,123n,'1e-8','\ud800'])cases.push(['numberToString',[x]]);
for(const x of [undefined,null,true,0,1,3,-2,'2.9','0x10','garbage','\ud800'])cases.push(['parsePrecision',[x]]);
for(const x of ['12.3456','-12.3456','0.00098765','999.999','1.45','0','-0','','--1','1..2','1e-8','\ud800',undefined,null,NaN,Infinity,123n])for(const mode of [0,1])for(const count of [2,3])cases.push(['decimalToPrecision',[x,mode,2,count,5]]);
for(const args of [['1.2',1,undefined,2,6],['1.2',1,'bad',2,6],['1.2',1,null,2,6],['1.2',1,2.5,2,6],['123.45',1,-0.5],['1.235',2,2],['1.235',3,2],['1.235',1,123n],['123.45',1,-1],['123.45',0,-1],['1.2345',1,2.5],['1.2345','1',2],['1.2345',1,'2'],['1.2345',1,null],['1.2345',1,undefined],['1.2345',1,2,2,6],['1.2345',1,.05,4,5],['-1.2345',1,'0.05junk',4,5],['1',1,0,4,5],['1',1,-1,4,5]])cases.push(['decimalToPrecision',args]);
for(const x of [undefined,null,'',0,-0,NaN,Infinity,true,123n,'\ud800'])cases.push(['safeString',[{x},'x','default']]);
function encode(v){if(v===undefined)return{special:'undefined'};if(typeof v==='number'&&!Number.isFinite(v))return{special:Number.isNaN(v)?'nan':v>0?'infinity':'negativeInfinity'};if(Object.is(v,-0))return{special:'negativeZero'};if(typeof v==='bigint')return{special:'bigint'};if(v==='\ud800')return{special:'surrogate'};if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)]));return v;}
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const encoded=cases.map(([name,args])=>({name,args:args.map(encode),expected:(()=>{try{const v=api[name](...args);return v===undefined?{kind:'undefined'}:{kind:'return',value:typeof v==='string'?units(v):v};}catch(e){return{kind:'throw',name:e.name,message:units(e.message)};}})()}));
console.log(JSON.stringify(encoded));
