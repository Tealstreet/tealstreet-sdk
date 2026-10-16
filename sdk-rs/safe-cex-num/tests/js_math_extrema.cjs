const buffer = Buffer.alloc(8);
const {bits} = require('./support/number-bits.cjs');
function value(n) {buffer.writeBigUInt64BE(BigInt.asUintN(64,n));return buffer.readDoubleBE();}
const special=[NaN,1,-1,-0,0,Infinity,-Infinity,Number.MAX_VALUE,Number.MIN_VALUE,-Number.MIN_VALUE,Number('\ufeff-0\ufeff')];
const rows=[];
const add=(a,b)=>rows.push([bits(a),bits(b),bits(Math.min(a,b)),bits(Math.max(a,b))]);
for(const a of special)for(const b of special)add(a,b);
let state=42n;
function random(){state=BigInt.asUintN(64,state*6364136223846793005n+1442695040888963407n);return value(state);}
for(let i=0;i<30000;i++){const a=random(),b=random();add(a,b);add(b,a);if(i%10===0)add(a,a);}
console.error(JSON.stringify({node:process.version,cases:rows.length}));
process.stdout.write(JSON.stringify(rows));
