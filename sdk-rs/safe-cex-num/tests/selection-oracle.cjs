const fs = require('node:fs');
const {stripTypeScriptTypes} = require('node:module');
const root = require('node:path').resolve(__dirname, '../../..');
function load(path,name) {
 return new Function(stripTypeScriptTypes(fs.readFileSync(root+'/'+path,'utf8').replace(/export /g,''))+'\nreturn '+name+';')();
}
const cursor = load('packages/safe-cex/src/utils/oldestCursorId.ts','oldestCursorId');
const sizing = load('packages/script-api-core/src/order/positionSizing.ts','nativePositionSize');
const encode = x => Number.isNaN(x)?'NaN':Object.is(x,-0)?'-0':String(x);
const decode = x => x === null ? undefined : Number(x);
let cases=[];
const pages = [[],['001','2'],['9007199254740993','9007199254740992'],['9'.repeat(400),'1'.repeat(399)],[' 123 ','\ufeff122\ufeff','\u0085121\u0085'],['-1','1.0','١',''],[null,true,{},['12'],[[11]],[null],12],['100','99','98'],['0','00'],['1e3',1000],['9'.repeat(1000),'8'.repeat(1000)]];
for(const ids of pages) for(const ordered of [ids,[...ids].reverse()]) {
 const rows=ordered.map(id=>({billId:id})); rows.push({},null);
 // Coercion belongs to the JS boundary, before JSON transport of string IDs.
 cases.push({op:'cursor',ids:ordered.filter(id=>id!=null).map(String),expected:cursor(rows,'billId')??null});
}
const values=[null,'0','-0','NaN','Infinity','-Infinity','-100','0.01','1','100'];
for(const contracts of values) for(const notional of values) for(const native of values) for(const size of values) for(const inverse of [false,true]) {
 const p={contracts:decode(contracts),notional:decode(notional),extra:{contractsNative:decode(native)}};
 const m={contractSize:decode(size),isInverse:inverse};
 cases.push({op:'size',contracts,notional,native,size,inverse,expected:encode(sizing(p,m))});
}
process.stdout.write(JSON.stringify(cases));
