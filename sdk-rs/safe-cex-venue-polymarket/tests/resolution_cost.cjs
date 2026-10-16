// Execute actual private members through the existing host; do not rewrite bodies.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const family=process.argv[2];
const families={exchange:['exchange','mapOrder'],private:['private','mapOrder'],update:['private','handleOrderUpdate']};
assert(Object.hasOwn(families,family),'explicit recovered family');
const lengths=[0,1,2,3,15,16,17,31,32,33,63,64,65,66,127,128,129,255,256,257,511,512,513,1023,1024,1025,47,48,49,95,96,97];
const prices=['0','-0','1','-1','.5','1e-4000','-1e-4000','1e309','-1e309','NaN','Infinity','-Infinity','0x10','0.125tail','',' ','\u00a00.25tail','1e-323','5e-324','2.2250738585072014e-308','1.7976931348623157e308','9007199254740991','9007199254740992','9007199254740993','1e2x','.','+','1e-2','0.0000000000000001','-0tail','3.4028234663852886e38','1.0000000000000002'];
assert.equal(lengths.length,32);assert.equal(prices.length,32);
let code=fs.readFileSync(path.join(__dirname,'order_adapters_oracle.cjs'),'utf8');
function replace(before,after){assert.equal(code.split(before).length,2,'unique host hook '+before.slice(0,80));code=code.replace(before,after);}
replace("async function fixture(venue,name,i,abandon=false,ready=false){",`async function fixture(venue,name,i,abandon=false,ready=false,generation=0,iteration=0){`);
// i retains the original independent exception/getter/alias protocols. The
// separately declared generation supplies input values, never expected outputs.
replace("const o={marker};for(const [key,initial]of Object.entries(raw))",`if(![2,3].includes(i))raw.side=generation%2?'SELL':'BUY';
raw.id='x'.repeat(lengths[generation]);raw.asset_id='asset-'+iteration;
if(![4,7,10].includes(i)&&!([6,11].includes(i)&&name!=='handleOrderUpdate')&&i!==11)raw.price=prices[generation];
if(i!==8)raw.original_size=String(3+generation)+'tail';raw.size_matched=String(1+generation%3);
const o={marker};for(const [key,initial]of Object.entries(raw))`);
const start=code.indexOf('(async()=>{const cases=[];');assert(start>0,'single host entrypoint');
code=code.slice(0,start)+`(async()=>{const cases=[];const [venue,name]=families[family];
for(let iteration=0;iteration<512;iteration++){
 const generation=Math.floor(iteration/16),i=(iteration%16)%12;
 const c=await fixture(venue,name,i,false,false,generation,iteration);
 cases.push({...c,generation,iteration,inputTextLength:lengths[generation]});
}
assert.equal(cases.length,512,'independent512 case floor');assert.equal(cases[511].iteration,511);
assert.equal(cases[192].inputTextLength,65,'generation12 reaches65 before comparison');
process.stdout.write(JSON.stringify({cases}));})().catch(e=>{console.error(e);process.exitCode=1;});`;
new Function('require','__dirname','families','family','lengths','prices',code)(require,__dirname,families,family,lengths,prices);
