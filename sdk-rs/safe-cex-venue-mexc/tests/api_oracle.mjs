import {load} from './source.mjs';
const api=load('packages/safe-cex/src/exchanges/mexc/mexc.api.ts');
const encode=v=>v===undefined?{$undefined:true}:typeof v==='bigint'?{$bigint:String(v)}:typeof v==='number'&&(!Number.isFinite(v)||Object.is(v,-0))?{$number:Object.is(v,-0)?'-0':String(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
const cases=[];
const materialize=v=>v&&typeof v==='object'&&v.$entries?Object.fromEntries(v.$entries.map(([k,value])=>[k,materialize(value)])):Array.isArray(v)?v.map(materialize):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,value])=>[k,materialize(value)])):v;
function run(name,args){try{cases.push({name,args:encode(args),output:api[name](...args.map(materialize))});}catch(e){cases.push({name,args:encode(args),error:{name:e.name,message:e.message}});}}
for(const secret of ['', 'secret','\uFEFF秘密😃','a'.repeat(160)])for(const params of ['', 'a=1&z=2', '{"z":1,"a":2}', '["9007199254740993123"]', 'raw body string'])run('signMexcRequest',[secret,'key','1791800000123',params]);
for(const key of ['','key','\n','ключ'])run('signMexcWsLogin',['secret',key,'1791800000123']);
for(const url of [undefined,'','/api/v1/contract/detail','/api/v1/contract/detail?x=1','/api/v1/contract/detail/BTC_USDT','/api/v1/contract/details','https://api.mexc.com/api/v1/contract/detail','/api/v1/contract/ticker','/api/v1/contract/depth/BTC_USDT','/api/v1/contract/kline/BTC_USDT?x=1','/api/v1/private/order/submit',' /api/v1/contract/detail','/API/v1/contract/detail','?x=/api/v1/contract/detail'])run('isMexcPublicEndpoint',[url]);
for(const body of [undefined,null,'','raw body string','{"z":1,"a":2}',false,true,0,-0,1e21,1e-7,Infinity,NaN,[1,undefined,null,NaN],{z:1,a:undefined,b:null},{z:1,2:'two',1:'one',a:2}, {"key":"\uFEFF😃\n"}, 1n, {value:1n}, [1n]])run('normalizeRequestData',[body]);
const entryCases=[
 {$entries:[['a',1],['a',2]]},
 {$entries:[['a',1],['a',undefined]]},
 {$entries:[['é',1],['e\u0301',2],['é',3]]},
 {$entries:[['2',1],['10',2],['2',3]]},
 {$entries:[['__proto__',1],['constructor',2],['__proto__',3]]},
 {x:{$entries:[['z',1],['a',2],['z',3]]}},
];
const queryValues=[...entryCases,undefined,null,'abc',0,false,[],{},NaN,Infinity,-Infinity,-0,0n,1n,-1n,10n**400n,JSON.parse('{"__proto__":{"z":2,"a":1},"constructor":1}'), {z:1,a:2}, {Z:1,a:2,A:3,ä:4,'é':5,e:6,'中':7,'𠀀':8},
 {x:[1,2,null,undefined,'',false]}, {x:Array.from({length:12},(_,i)=>i)},
 {x:[{z:1,a:2},{a:3}], n:{z:[1,2],a:{k:4}}},
 {x:[[],{},[1,2]], 'a[b]':'x y+z&', 'a.b':'!*()~_-'},
 {x:Infinity,y:NaN,z:-0,n:1e21,t:1e-7,b:123456789012345678901n},
 {x:'\uFEFF秘密😃\n',e:'e\u0301', 'e\u0301':1,'é':2},
 Object.assign(Object.create(null), {'__proto__':null,constructor:1,toString:2}),
 ['a','b',null,undefined,{b:2,a:1}],
];
for(const params of queryValues){run('serializeMexcParams',[params]);for(const url of [undefined,'/private?a=1?b=2','/private?raw=%20&x=1#hash'])run('getQueryString',[{params,url}]);}
process.stdout.write(JSON.stringify(cases.map(item=>({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),...item}))));
