// Reuse the actual-source loader, never a copied mapper body.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
const file = path.join(__dirname, 'utilities-oracle.cjs');
const loader = fs.readFileSync(file, 'utf8');
const end = loader.indexOf("const income = load(");
if (end < 0) throw Error('maintained module loader boundary missing');
vm.runInNewContext(loader.slice(0, end) + `
function capture(value) {
 if (typeof value === 'number') { const b=Buffer.alloc(8);b.writeDoubleBE(value);return {bits:b.toString('hex')}; }
 if (typeof value === 'string') return {units:Array.from({length:value.length},(_,i)=>value.charCodeAt(i))};
 if (value === undefined) return {undefined:true};
 if (Array.isArray(value)) return value.map(capture);
 if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,capture(v)]));
 return value;
}
const base={fill_id:'first',symbol:'pf_xbtusd',fillTime:'2023-11-14T22:13:19Z',side:'BUY',order_id:'order-A',price:'30001.25',size:'2.5',fillType:'MAKER',realized_pnl:'-3.75'};
let deep=0;for(let i=0;i<150;i++)deep=[deep];
const cases=[];
function add(label,patch) {
 const rows=[{...base,...patch},{...base,fill_id:'neighbor',side:'SELL',fillType:'taker',price:'30002.5',size:'4.25',realized_pnl:0}];
 const raw=JSON.stringify({result:'success',fills:rows});
 const calls=[];
 const output=fills.mapKrakenFills(JSON.parse(raw).fills,s=>{calls.push(s);return s.toUpperCase()==='PF_XBTUSD'?'PF_XBTUSD':undefined;});
 cases.push({label,raw,output:capture(output),calls});
}
add('ignored-high',{ignored:String.fromCharCode(0xd800)});
add('ignored-low',{ignored:String.fromCharCode(0xdc00)});
add('ignored-pair',{ignored:String.fromCodePoint(0x1f4b0)});
add('ignored-depth150',{ignored:deep});
add('typed-id',{fill_id:String.fromCharCode(0xd800),order_id:String.fromCharCode(0xdc00)});
add('zero-numbers',{price:'-0',size:'-0',realized_pnl:'-0'});
add('nonfinite-fields',{price:'Infinity',size:'-Infinity',realized_pnl:'Infinity'});
add('null-presence',{order_id:0,realized_pnl:null});
add('empty-presence',{order_id:false,realized_pnl:''});
add('primitive-row',{});
// Keep numeric tokens before JSON.stringify's nonfinite projection.
for(const [label,token] of [['ignored-overflow','1e400'],['ignored-negative-zero','-0']]) {
 const raw=JSON.stringify({result:'success',fills:[{...base,ignored:'TOKEN'},{...base,fill_id:'neighbor'}]}).replace('"TOKEN"',token);
 cases.push({label,raw,output:capture(fills.mapKrakenFills(JSON.parse(raw).fills,s=>'PF_XBTUSD'))});
}
process.stdout.write(JSON.stringify({caseCount:12,cases}));
`, {require:createRequire(file),__dirname,Buffer,process,console},{filename:file});
