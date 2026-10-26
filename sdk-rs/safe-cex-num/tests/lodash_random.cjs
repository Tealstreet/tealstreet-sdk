const assert = require('node:assert/strict');
const fs = require('node:fs');
let { lodash } = require('./lodash-profile.cjs');
let draw=0,calls=0;
const math=Object.create(Math);
math.random=()=>{calls++;return draw;};
if (process.argv.includes('--control-integer-span')) {
  const { entry } = require('./lodash-profile.cjs');
  const source=fs.readFileSync(entry,'utf8');
  const wrong=source.replace('nativeRandom() * (upper - lower + 1)','nativeRandom() * (upper - lower)');
  assert.notEqual(source,wrong,'integer-span mutation must apply');
  const sandbox={module:{exports:{}},exports:{}};sandbox.exports=sandbox.module.exports;
  require('node:vm').runInNewContext(wrong,sandbox);lodash=sandbox.module.exports;
}
const fixed=lodash.runInContext({Math:math});
const {bits} = require('./support/number-bits.cjs');
function number(hex) { return Buffer.from(hex,'hex').readDoubleBE(); }
if (process.argv.includes('--surrogate-witnesses')) {
  draw=0.5;
  const raws=['"\\ud800"','"1\\udfff"','"\\ud83d\\ude00"','"\\ufeff2"','"\\u00852"','"2"'];
  process.stdout.write(JSON.stringify(raws.map(raw=>({raw,bits:bits(fixed.random(JSON.parse(raw),2))}))));
  process.exit(0);
}

function decode(v) {
  switch(v[0]) {
    case 'undefined':return undefined;
    case 'number':return number(v[1]);
    case 'bigint':return BigInt(v[1]);
    case 'symbol':return Symbol('bound');
    case 'json':return v[1];
    default:throw Error('bad tag');
  }
}
const prim=[['undefined'],['json',null],['json',false],['json',true],['symbol'],['bigint','0'],['bigint','1'],['bigint','-2'],
 ...[0,-0,1,-1,2,-2,5,1.2,-1.2,Number.MIN_VALUE,Number.MAX_VALUE,-Number.MAX_VALUE,Infinity,-Infinity,NaN].map(n=>['number',bits(n)]),
 ...['',' ','\ufeff2','\u00852','0x10','-0x1','0b11','-2.5','NaN','Infinity','1e400','text'].map(s=>['json',s])];
const json=[['undefined'],... [null,false,true,0,-0,1,2,3.5,-2,'','\ufeff2','text',[],[2],[[3]],{}, {valueOf:3}, {toString:4}, {length:2}, {'0':3,length:2}, [3,4]].map(v=>typeof v === 'number' ? ['number',bits(v)] : ['json',v])];
const draws=[0,-0,Number.MIN_VALUE,1e-7,1e-6,0.1,0.25,0.5,0.9,0.9999999999999999,1];
let count=0,buffer='';
function flush(){fs.writeSync(1,buffer);buffer='';}
function emit(mode,args,value,alias=null){
  draw=value;calls=0;
  let decoded=args.map(decode);
  if(alias!==null) decoded[0]=decoded[2][alias];
  let result;
  try { result={bits:bits(fixed.random(...decoded))}; } catch(e) { result={error:e.name}; }
  buffer+=JSON.stringify({mode,args,draw:bits(draw),alias,calls,...result})+'\n';count++;
  if(buffer.length>32768)flush();
}
for(const low of prim)for(const high of prim)for(const flag of [['undefined'],['json',false],['json',true],['json',''],['json','yes'],['symbol'],['bigint','0']])for(const sample of draws)emit('primitive',[low,high,flag],sample);
for(const low of json)for(const high of json)for(const flag of [['undefined'],['json',false],['json',true],['json',[3,4]],['json',{'0':3,length:2}],['json',{k:3}]])for(const sample of [0,0.5,0.9999999999999999])emit('json',[low,high,flag],sample);
for(const index of [0,1,'0','length'])for(const sample of draws)emit('json',[['undefined'],['json',index],['json',[[1],3]]],sample,index);
let state=0xcef080ad;
function random(){state^=state<<13;state^=state>>>17;state^=state<<5;return state>>>0;}
for(let i=0;i<20000;i++){
 const bound=()=>{const b=Buffer.alloc(8);b.writeUInt32BE(random());b.writeUInt32BE(random(),4);return ['number',b.toString('hex')];};
 emit('primitive',[bound(),bound(),['json',!!(i%2)]],random()/4294967296);
}
flush();console.error(JSON.stringify({count,node:process.version,lodash:lodash.VERSION}));
