'use strict';
// Reuse the maintained loader; all mapper bodies are the actual TypeScript source.
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const {createRequire} = require('node:module');
const file = path.join(__dirname, 'utilities-oracle.cjs');
const loader = fs.readFileSync(file, 'utf8');
const end = loader.indexOf('const income = load(');
if (end < 0) throw Error('maintained loader boundary missing');
vm.runInNewContext(loader.slice(0, end) + `
const income = load(path.join(src, 'exchanges/kraken/kraken.income.ts'));
function capture(value) {
 if (typeof value === 'number') { const b=Buffer.alloc(8); b.writeDoubleBE(value); return {bits:b.toString('hex')}; }
 if (value === undefined) return {undefined:true};
 if (Array.isArray(value)) return value.map(capture);
 if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,capture(v)]));
 return value;
}
const fillsCases=[], incomeCases=[];
const fill={fill_id:'fill-A',symbol:'pf_xbtusd',fillTime:'2023-11-14T22:13:19.750Z',side:'BUY',order_id:'order-A',price:'30001.25',size:'2.5',fillType:'MAKER',realized_pnl:'-3.75'};
for (const [label,patch] of [
 ['finite',{price:'-0',size:[3.25],realized_pnl:'-0'}],
 ['null',{price:null,size:false,realized_pnl:null}],
 ['missing',{price:undefined,size:undefined,realized_pnl:undefined}],
 ['nonfinite',{price:Infinity,size:NaN,realized_pnl:-Infinity}],
 ['optional-empty',{price:true,size:[],realized_pnl:''}],
 ['date-array',{fillTime:['2023-11-14T22:13:19.750Z']}],
 ['invalid-date',{fillTime:'not a date'}],
]) {
 const rows=[{...fill,...patch},{...fill,fill_id:'neighbor'}];
 fillsCases.push({label,rows:capture(rows),output:capture(fills.mapKrakenFills(rows,s=>undefined))});
}
const log={date:'2023-11-14T22:13:19.750Z',booking_uid:'booking-A',contract:'pf_xbtusd',asset:'usd',info:'futures trade',realized_pnl:'4.25',realized_funding:'-1.5',fee:'0.75',liquidation_fee:'0.125'};
for (const [label,patch] of [
 ['finite',{realized_pnl:[3.25],fee:'-0'}],
 ['null',{realized_pnl:null,realized_funding:'',fee:false,liquidation_fee:0}],
 ['missing',{realized_pnl:undefined}],
 ['nonfinite',{realized_pnl:NaN,realized_funding:Infinity,fee:-Infinity}],
 ['date-array',{date:['2023-11-14T22:13:19.750Z']}],
 ['invalid-date',{date:'not a date'}],
 ['transfer',{realized_pnl:null,realized_funding:null,fee:null,liquidation_fee:null,info:'deposit',old_balance:'7.25',new_balance:'9.75'}],
]) {
 const rows=[{...log,...patch},{...log,booking_uid:'neighbor'}];
 incomeCases.push({label,rows:capture(rows),output:capture(income.mapKrakenAccountLog(rows,s=>undefined))});
}
process.stdout.write(JSON.stringify({fillsCases,incomeCases}));
`, {require:createRequire(file),__dirname,Buffer,process,console},{filename:file});
