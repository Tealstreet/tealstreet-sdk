const {bits} = require('./support/number-bits.cjs');
const b=Buffer.alloc(8), value=n=>{b.writeBigUInt64BE(BigInt.asUintN(64,n));return b.readDoubleBE();};
const xs=[100.00000000000006,9.999999999999672e-254,NaN,Infinity,-Infinity,0,-0,-1,Number('\ufeff100\ufeff')];
for(let e=-324;e<=308;e++) { const base=BigInt('0x'+bits(Number('1e'+e))); for(let d=-256n;d<=256n;d++) { const n=base+d; if(n>=0n && n<=0x7fefffffffffffffn) xs.push(value(n)); } }
let state=42n;for(let i=0;i<10000;i++) {state=BigInt.asUintN(64,state*6364136223846793005n+1442695040888963407n);xs.push(value(state));}
process.stderr.write(JSON.stringify({arch:process.arch,platform:process.platform,node:process.version,v8:process.versions.v8,cases:xs.length})+'\n');
process.stdout.write(JSON.stringify(xs.map(x=>[bits(x),bits(Math.log10(x))])));
