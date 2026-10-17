const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleLE(n);return b.readBigUInt64LE().toString(16).padStart(16,'0')};
const read=h=>{const b=Buffer.alloc(8);b.writeBigUInt64LE(BigInt('0x'+h));return b.readDoubleLE()};
const values=[255,1.5,0.1,0.9999999999999999,1.0000000000000002,35.99999999999999,Number.MIN_VALUE,Number.MAX_VALUE,Number.MIN_SAFE_INTEGER,Number.MAX_SAFE_INTEGER,2**53,2**54,1e100,1e-100,-255,-1.5,-0.1,-Number.MIN_VALUE,-Number.MAX_VALUE,0,-0,NaN,Infinity,-Infinity].map(bits);
let seed=0x93d128b91a53c4fen;const mask=(1n<<64n)-1n;
for(let i=0;i<128;i++){seed^=seed<<13n;seed&=mask;seed^=seed>>7n;seed^=seed<<17n;seed&=mask;values.push(seed.toString(16).padStart(16,'0'))}
const cases=[];
for(const bits of values)for(let radix=2;radix<=36;radix++)cases.push({bits,radix,result:read(bits).toString(radix)});
const radixInputs={omitted:()=>[],undefined:()=>[undefined],null:()=>[null],zero:()=>[0],one:()=>[1],thirty_seven:()=>[37],nan:()=>[NaN],infinity:()=>[Infinity],negative_infinity:()=>[-Infinity],fraction:()=>[2.9],string:()=>['16'],object:()=>[{valueOf(){return 16}}],throwing:()=>[{valueOf(){throw new RangeError('radix coercion')}}],symbol:()=>[Symbol.iterator],bigint:()=>[16n]};
const conversions=[];
for(const number of [255,0,NaN,Infinity])for(const [tag,args] of Object.entries(radixInputs)){
 let result=null,error=null;try{result=number.toString(...args())}catch(e){error=e.name}
 conversions.push({bits:bits(number),tag,result,error});
}
process.stdout.write(JSON.stringify({cases,conversions}));
