const B = require('bignumber.js');
const specs = [
 ['huge_finite','string','1e1000'],['negative_huge_finite','string','-1e1000'],['tiny_finite','string','1e-1000'],['negative_tiny_finite','string','-1e-1000'],
 ['tiny','number','-0.004'],['negative_zero','number','-0'],['zero','number','0'],
 ['positive','number','1.25'],['negative','number','-2.5'],
 ['infinity','number','Infinity'],['negative_infinity','number','-Infinity'],['nan','number','NaN'],
 ['negative_zero_string','string','-0'],['infinity_string','string','Infinity'],
 ['negative_infinity_string','string','-Infinity'],['nan_string','string','NaN'],
 ['negative_underflow','string','-0.00000000000000000001'],
 ['precise','string','12345678901234567890.125'],['decimal','string','1.5'],
 ['negative_decimal','string','-1.5'],['exponent','string','1e+21'],['small_exponent','string','1e-8'],
].map(([tag,kind,value])=>({tag,kind,value}));
const input = s => s.kind==='number'?Number(s.value):s.value;
const number = n => Object.is(n,-0)?'-0':String(n);
const view = n => ({str:n.toString(),value:n.valueOf(),fixed:[n.toFixed(),n.toFixed(0),n.toFixed(2),n.toFixed(6)],json:JSON.stringify({sz:n}),nan:n.isNaN(),finite:n.isFinite(),number:number(n.toNumber())});
const scalars=specs.map(spec=>{const n=new B(input(spec));return {spec,result:view(n),clone:view(new B(n))}});
const operations=['plus','minus','times','multipliedBy','idiv','dividedToIntegerBy'];
const comparisons=['lt','lte','gt','gte','eq','isEqualTo'];
const selected=['negative_infinity','negative','negative_zero','zero','positive','infinity','nan'].map(tag=>specs.find(s=>s.tag===tag));
const pairs=[];
for(const left of selected)for(const right of selected){
 const n=new B(input(left));const other=input(right);
 pairs.push({left,right,operations:Object.fromEntries(operations.map(k=>[k,view(n[k](other))])),comparisons:Object.fromEntries(comparisons.map(k=>[k,n[k](other)]))});
}
const extremes=[];
for(const args of [['infinity','positive'],['negative_infinity','negative'],['nan','positive'],['negative_zero','zero'],['zero','negative_zero']]){
 const inputs=args.map(tag=>specs.find(s=>s.tag===tag));
 extremes.push({inputs,result:Object.fromEntries(['min','minimum','max','maximum'].map(k=>[k,view(B[k](...inputs.map(input)))]))});
}
process.stdout.write(JSON.stringify({scalars,pairs,extremes}));
