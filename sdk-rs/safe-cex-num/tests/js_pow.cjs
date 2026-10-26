const {bits} = require('./support/number-bits.cjs');
const b=Buffer.alloc(8), value=n=>{b.writeBigUInt64BE(BigInt.asUintN(64,n));return b.readDoubleBE();};
const cases=[[Number.MAX_VALUE,0.5]],special=[NaN,Infinity,-Infinity,0,-0,1,-1,2,-2,0.5,-0.5,Number.MIN_VALUE,Number.MAX_VALUE];
for(const x of special) for(const y of special) cases.push([x,y]);
const ys=[-1075,-1024,-53,-3,-2,-1,-0.5,0,0.5,1,2,3,10,53,1024,2**31,2**32,2**64];
for(let e=-1074;e<=1023;e++) {
 const base=BigInt('0x'+bits(2**e));
 for(let d=-2n;d<=2n;d++) {
  const n=base+d;if(n<0n||n>0x7fefffffffffffffn)continue;
  for(const y of ys)cases.push([value(n),y]);
 }
}
let state=42n;for(let i=0;i<20000;i++) {state=BigInt.asUintN(64,state*6364136223846793005n+1442695040888963407n);const x=value(state);state=BigInt.asUintN(64,state*6364136223846793005n+1442695040888963407n);cases.push([x,value(state)],[x,(i%100-50)/7]);}
cases.push([Number('\ufeff10\ufeff'),2]);
process.stderr.write(JSON.stringify({arch:process.arch,platform:process.platform,node:process.version,v8:process.versions.v8,cases:cases.length})+'\n');
process.stdout.write(JSON.stringify(cases.map(([x,y])=>[bits(x),bits(y),bits(Math.pow(x,y))])));
