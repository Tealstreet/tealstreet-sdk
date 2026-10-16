const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const {root,ts,header,capture,load}=require('./types-oracle.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['toNumber','normalizeOrderBookSide','normalizeOrderBook'];
const selected=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
assert.equal(selected.length,3,'book_source_inventory');
const context={exports:{},...load(path.join(root,'packages/safe-cex/src/utils/orderbook.ts'))};
vm.runInNewContext(ts.transpileModule(selected.map(n=>n.getText(ast)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
const sides=[['absent',undefined],['null',null],['false',false],['zero',0],['empty_string',''],['empty',[]],['distinct',[['107.5','2.25'],['101.25','3.5'],['109.75','4.125']]],['signed_zero',[['1','-0'],['2','-1e-4000']]],['filter',[['0','7'],['-3','2'],['9','-2'],['9','oops'],['8','0'],['7','1.2tail'],['Infinity','4'],['2','Infinity']]],['precision',[['9007199254740993.0','0.1'],['0.000000001','0.2'],['3','0.3']]],['overflow',[['3','1e308'],['4','1e308']]],['short',[[],['5'],['6','2','extra']]],['string_rows',['12','345','1𝟚','𝟙2']],['json_values',[[4,3],[true,false],[['7'],['8']],[null,2]]],['nonarray',{}],['truthy_number',3],['truthy_string','12'],['null_row',[null]],['object_row',[{}]],['number_row',[3]],['boolean_row',[true]],['zero_price_bad_amount',[['0',{toString:null}]]],['bad_coercion',[[{'toString':null},'2']]]];
const books=[['absent',undefined],['distinct',{bids:[['107.5','2.25'],['101.25','3.5']],asks:[['113.75','7.125'],['111.125','11.5']]}],['aliases',{b:[['10','3']],a:[['20','7']]}],['precedence',{bids:[['11','2']],b:[['77','99']],asks:[['13','5']],a:[['88','66']]}],['nullish',{bids:null,b:[['11','3']],asks:null,a:[['20','7']]}],['falsey',{bids:false,b:[['11','3']],asks:0,a:[['20','7']]}],['missing',{}],['null',null],['array',[]],['primitive',7],['error_order',{bids:[[{toString:null},'2']],asks:[null]}],['errors',{bids:[null],asks:[['2','3']]}],['overflow',{bids:[['4','1e308'],['5','1e308']],asks:[['6','1'],['7','2']]}],['signed_zero',{bids:[['1','-0'],['2','-0']],asks:[['3','0']]}]];
const cases=[];
function record(fn,id,value,raw){
 try {
  const native=context.exports[fn](value);
  const levels=rows=>rows.map(r=>({price:capture(r.price),amount:capture(r.amount),total:capture(r.total)}));
  cases.push({fn,id,raw,result:fn==='normalizeOrderBookSide'?levels(native):{bids:levels(native.bids),asks:levels(native.asks)}});
 } catch(e) {cases.push({fn,id,raw,error:{name:e.name,message:e.message}});}
}
for(const [fn,inputs] of [['normalizeOrderBookSide',sides],['normalizeOrderBook',books]])
 for(const [id,value] of inputs) record(fn,id,value,value===undefined?null:JSON.stringify(value));
for(const [id,raw] of [['numeric_overflow_row','[1e400]'],['numeric_underflow_row','[-1e-4000]'],['rounded_row','[9007199254740993.0]'],['numeric_levels','[[1e400,3],[7,-1e-4000],[8,9007199254740993.0]]']])
 record('normalizeOrderBookSide',id,JSON.parse(raw),raw);
const refusals=[['normalizeOrderBookSide','[["\\ud800","2"]]'],['normalizeOrderBook','{"bids":[["\\ud800","2"]]}']].map(([fn,raw])=>({fn,raw,source:capture(context.exports[fn](JSON.parse(raw)))}));
const depth=[['normalizeOrderBookSide','[["5",'+ '['.repeat(150)+'"7"'+']'.repeat(150)+']]'],['normalizeOrderBook','{"bids":[["5",'+'['.repeat(150)+'"7"'+']'.repeat(150)+']]}']].map(([fn,raw])=>({fn,raw,source:capture(context.exports[fn](JSON.parse(raw)))}));
const callable=[['normalizeOrderBookSide',[[{toString:()=> '5'},'7']]],['normalizeOrderBook',{bids:[[{toString:()=> '5'},'7']]}]].map(([fn,value])=>({fn,source:capture(context.exports[fn](value))}));
process.stdout.write(JSON.stringify({header,cases,refusals,depth,callable}));
