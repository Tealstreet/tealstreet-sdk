// Offline measurement only: compares the host Rust libm against this Node/V8.
// It is not a parity claim or a production numeric implementation.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {execFileSync} = require('node:child_process');
const tmp = fs.mkdtempSync(path.join(os.tmpdir(),'lighter-powf-'));
const rust = `use std::io::{self,BufRead};
fn main() {
 for line in io::stdin().lock().lines() {
  let line=line.unwrap(); let mut fields=line.split_whitespace();
  let x=f64::from_bits(u64::from_str_radix(fields.next().unwrap(),16).unwrap());
  let y=f64::from_bits(u64::from_str_radix(fields.next().unwrap(),16).unwrap());
  let z=x.powf(y); if z.is_nan(){println!("NaN")}else{println!("{:016x}",z.to_bits())}
 }
}`;
const hex = x => {if(Number.isNaN(x))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(x);return b.toString('hex');};
const fromBits = bits => {const b=Buffer.alloc(8);b.writeBigUInt64BE(bits);return b.readDoubleBE();};
const cases=[];
const add=(kind,x,y)=>cases.push({kind,x,y});
const exponents=[-Infinity,-1075,-1074,-1024,-1023,-100,-30,-8,-3,-2,-1,-0.5,-Number.MIN_VALUE,-0,0,Number.MIN_VALUE,0.5,1,1.5,2,3,8,30,100,1023,1024,1074,1075,Infinity,NaN];
const bases=[-Infinity,-Number.MAX_VALUE,-10,-2,-1.5,-1,-Number.MIN_VALUE,-0,0,Number.MIN_VALUE,2**-1022,0.5,1,1.5,2,10,Number.MAX_VALUE,Infinity,NaN];
for(const x of bases)for(const y of exponents)add('specials',x,y);
for(let exponent=-1074;exponent<=1023;exponent++) {
 const base=2**exponent;
 for(const y of [-3,-2,-1,-0.5,0.5,1.5,2,3]) {add('binary-powers',base,y);add('negative-binary-powers',-base,y);}
}
for(let exponent=-323;exponent<=308;exponent++) {
 const base=10**exponent,bits=BigInt('0x'+hex(base));
 for(let offset=-8;offset<=8;offset++) {
  const near=bits+BigInt(offset);if(near<=0n)continue;
  const x=fromBits(near);
  for(const y of [-3,-0.5,0.5,1.5,2,3,8,30])add('decimal-neighbourhood',x,y);
 }
}
for(const x of [1.5,2,10,1+Number.EPSILON]) {
 const boundary=Math.log(Number.MAX_VALUE)/Math.log(x),bits=BigInt('0x'+hex(boundary));
 for(let offset=-256;offset<=256;offset++)add('overflow-neighbourhood',x,fromBits(bits+BigInt(offset)));
}
for(let count=0;count<=64;count++)add('backoff',1.5,Math.min(count,8));
for(let count=0;count<=128;count++)add('fractional-backoff',1.5,count/16);
for(let count=1;count<=8;count++){const bits=BigInt('0x'+hex(count));for(let offset=-256;offset<=256;offset++)add('backoff-neighbourhood',1.5,fromBits(bits+BigInt(offset)));}
for(let decimals=0;decimals<=30;decimals++)add('scaling',10,decimals);
let seed=0xa7189c21;
const rand=()=>{seed=(Math.imul(seed,1664525)+1013904223)>>>0;return seed;};
for(let i=0;i<20000;i++) {const x=fromBits((BigInt(rand()&0x7fffffff)<<32n)|BigInt(rand()));const y=(rand()/2**32-0.5)*64;add('generated',x,y);}
try {
 fs.writeFileSync(path.join(tmp,'probe.rs'),rust);
 execFileSync(process.env.RUSTC||'rustc',[path.join(tmp,'probe.rs'),'-O','-o',path.join(tmp,'probe')]);
 const outputs=execFileSync(path.join(tmp,'probe'),{input:cases.map(c=>`${hex(c.x)} ${hex(c.y)}\n`).join(''),maxBuffer:32*1024*1024}).toString().trim().split('\n');
 if(outputs.length!==cases.length)throw Error(`count ${outputs.length}/${cases.length}`);
 const mismatches=[],groups={};
 cases.forEach((c,i)=>{const expected=Math.pow(c.x,c.y),bits=Number.isNaN(expected)?'NaN':hex(expected);const group=groups[c.kind]??={cases:0,mismatches:0};group.cases++;if(bits!==outputs[i]){group.mismatches++;mismatches.push({kind:c.kind,base:String(c.x),exponent:String(c.y),baseBits:hex(c.x),exponentBits:hex(c.y),nodeBits:bits,rustBits:outputs[i]});}});
 const report={node:process.version,v8:process.versions.v8,platform:process.platform,arch:process.arch,rustc:execFileSync(process.env.RUSTC||'rustc',['--version']).toString().trim(),cases:cases.length,mismatches:mismatches.length,groups,witnesses:[...mismatches.slice(0,8),...mismatches.filter(c=>c.nodeBits!=='NaN'&&c.rustBits!=='NaN').slice(0,12)]};
 process.stdout.write(JSON.stringify(report,null,2)+'\n');
} finally {fs.rmSync(tmp,{recursive:true,force:true});}
