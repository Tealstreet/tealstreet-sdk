// Maintained source captures for Rust history helpers and mappers.
// All numeric/coercion/mapper behavior is executed from TypeScript, never reimplemented here.
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root=path.resolve(__dirname,'../../..');
function load(file,bindings={},extra='',enumsOnly=false) {
 let text=fs.readFileSync(path.join(root,file),'utf8');
 const mutation=process.env.BITGET_HISTORY_MUTATION;
 const mutations={
  'commission-abs':['bitget.fills.ts','commission: -fee,','commission: Math.abs(fee),'],
  'income-fee':['bitget.income.ts','income: fee,','income: -fee,'],
  'income-own-only':['bitget.income.ts',"CATEGORY_BY_TYPE[type] ?? 'other'","(Object.hasOwn(CATEGORY_BY_TYPE, type) ? CATEGORY_BY_TYPE[type] : undefined) ?? 'other'"],
 };
 if(mutation)assert.ok(Object.hasOwn(mutations,mutation),'known history mutation');
 if(mutation && file.endsWith(mutations[mutation][0])) {
  const [,from,to]=mutations[mutation];
  assert.equal(text.split(from).length,2,'history mutation source anchor');
  text=text.replace(from,to);
 }
 const ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
 const body=ast.statements.filter(n=>enumsOnly?ts.isEnumDeclaration(n):!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n')+extra;
 const exports={};
 new Function('exports',...Object.keys(bindings),ts.transpileModule(body,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(exports,...Object.values(bindings));
 return exports;
}
const enums=load('packages/safe-cex/src/types.ts',{},'',true);
const fills=load('packages/safe-cex/src/exchanges/bitget/bitget.fills.ts',enums,'\nexport {num,totalFeeOf,legAndDirectionOf};');
const income=load('packages/safe-cex/src/exchanges/bitget/bitget.income.ts',{},'\nexport {num};');
const encode=value=> {
 if(value===undefined)return {$undefined:true};
 if(typeof value==='number')return {$number:Object.is(value,-0)?'-0':String(value)};
 if(typeof value==='function')return {$function:String(value)};
 if(Array.isArray(value))return value.map(encode);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)]));
 return value;
};
const capture=fn=>{try{const value=fn();return {value:encode(value),json:encode(JSON.stringify(value))};}catch(error){return {error:{name:error.name,message:error.message}};}};
const values=[undefined,null,'',' ','\uFEFF','\u0085','\uFEFF12.5tail','\u008512.5','12.5tail','0x10','0b10','Infinity','-Infinity','1e309','-0',0,-0,1e21,1e-7,NaN,Infinity,false,true,[],[null],[12.5],[1,2],{}, {toString:0},[ {toString:0} ]];
const numbers=values.map(value=>({input:encode(value),fills:capture(()=>fills.num(value)),income:capture(()=>income.num(value))}));
const legCases=[];
for(const tradeSide of [undefined,null,'','open','close','open_long','close_long','open_short','close_short','buy_single','sell_single','nonsense_single','open_shortlong','close_long_short','reopen_long','OPEN','CLOSE_SHORT','\uFEFFopen','open\uFEFF','open\u0085',[],['open'],{}, {toString:0}])for(const side of [undefined,null,'','buy','sell','BUY','SELL','long','short',' buy','buy\uFEFF','sell\u0085',[],['buy'],{}, {toString:0}]){
 const row={tradeSide,side};legCases.push({input:encode(row),result:capture(()=>fills.legAndDirectionOf(row))});
}
const fillBase={symbol:'BTCUSDT',cTime:'1700000000123',baseVolume:'30',price:'0.33425',profit:'-0.0003',tradeSide:'close',side:'sell',tradeId:'trade-1',orderId:'order-1',feeDetail:[{feeCoin:'USDT',totalFee:'-0.02'}],tradeScope:'maker'};
function mapFill(row,mode='normal'){
 const calls=[];
 const deps={resolveSymbol:symbol=>{calls.push(['resolveSymbol',symbol]);if(mode==='resolveThrows')throw new TypeError('fixture resolver failure');return symbol==='BTCUSDT'?'BTC/USDT':symbol==='EMPTY'?'':undefined;},toTimestamp:ms=>{calls.push(['toTimestamp',ms]);if(mode==='timestampThrows')throw new TypeError('fixture timestamp failure');return ms/1000;}};
 const result=capture(()=>Array.isArray(row)&&mode==='batch'?fills.mapBitgetFills(row,deps):fills.mapBitgetFill(row,deps));
 return {input:encode(row),mode,result,calls:encode(calls)};
}
const fillCases=[];
for(const field of ['symbol','cTime','baseVolume','price','profit','tradeId','orderId','tradeScope','fee'])for(const value of values)fillCases.push({name:field+':'+fillCases.length,...mapFill({...fillBase,[field]:value,...(field==='fee'?{feeDetail:undefined}:{})})});
for(const detail of [undefined,null,{},[],[null],[{}],[{totalFee:'-0.02',feeCoin:null}],[{totalFee:'-0.02',feeCoin:[]}],[{totalFee:'-0.02'},{totalFee:'0.01'}],[{totalFee:'1e308'},{totalFee:'1e308'}],[{totalFee:{toString:0}}],[{totalFee:'-1',feeCoin:{toString:0}}]])fillCases.push({name:'feeDetail:'+fillCases.length,...mapFill({...fillBase,feeDetail:detail})});
for(const detail of [{'0':{feeCoin:'BGB',totalFee:'-7'}},{'0':{feeCoin:{toString:0}}}])fillCases.push({name:'objectFeeDetail:'+fillCases.length,...mapFill({...fillBase,feeDetail:detail,fee:'-2'})});
for(const tradeSide of ['open','close','open_long','close_short','buy_single','bad','','CLOSE'])for(const side of ['buy','sell','bad',null])fillCases.push({name:'direction:'+fillCases.length,...mapFill({...fillBase,tradeSide,side})});
for(const row of [undefined,null,{}, {...fillBase,symbol:'EMPTY'}, {...fillBase,symbol:'UNKNOWN',cTime:{toString:0}}, {...fillBase,baseVolume:0,feeDetail:[{totalFee:{toString:0}}]}])fillCases.push({name:'guard:'+fillCases.length,...mapFill(row)});
for(const mode of ['resolveThrows','timestampThrows'])fillCases.push({name:mode,...mapFill(fillBase,mode)});
fillCases.push({name:'batch_drop_order',...mapFill([fillBase,{...fillBase,symbol:'UNKNOWN'},{...fillBase,tradeId:'second'}],'batch')});
fillCases.push({name:'batch_throws_before_later_row',...mapFill([fillBase,{...fillBase,tradeId:{toString:0}},{...fillBase,tradeId:'unreached'}],'batch')});
const billBase={cTime:'1700000000123',businessType:'close_short',billId:'bill-1',coin:'USDT',symbol:'BTCUSDT',amount:'12.5',fee:'-0.2'};
const billCases=[];
for(const field of ['cTime','businessType','billId','coin','symbol','amount','fee'])for(const value of values){const row={...billBase,[field]:value};billCases.push({name:field+':'+billCases.length,input:encode(row),result:capture(()=>income.mapBitgetBill(row))});}
for(const type of ['buy','sell','open_long','open_short','close_long','close_short','contract_settle_fee','trans_from_exchange','trans_to_exchange','transfer_in','transfer_out','unknown','constructor','__proto__','toString','valueOf','hasOwnProperty','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toLocaleString'])for(const [amount,fee]of [[0,0],[1,0],[0,-1],[1,-1],[-2,1]]){
 const row={...billBase,businessType:type,amount,fee};billCases.push({name:'category:'+billCases.length,input:encode(row),result:capture(()=>income.mapBitgetBill(row))});
}
for(const row of [undefined,null,{}, {...billBase,cTime:0,coin:{toString:0}}, {...billBase,businessType:'',coin:{toString:0}}])billCases.push({name:'guard:'+billCases.length,input:encode(row),result:capture(()=>income.mapBitgetBill(row))});
const billBatches=[[billBase,{...billBase,billId:'next'}],[{...billBase,cTime:0},billBase],[billBase,{...billBase,coin:{toString:0}},{...billBase,billId:'unreached'}]].map(rows=>({input:encode(rows),result:capture(()=>income.mapBitgetBills(rows))}));
const feeCases=[undefined,null,{}, {fee:'-2'}, {feeDetail:[],fee:'-2'}, ...fillCases.filter(c=>c.name.startsWith('feeDetail:')).map(c=>decodeForFee(c.input))].map(row=>({input:encode(row),result:capture(()=>fills.totalFeeOf(row))}));
function decodeForFee(v){if(v&&typeof v==='object'){if(v.$undefined)return undefined;if('$number' in v)return Number(v.$number);if(Array.isArray(v))return v.map(decodeForFee);return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,decodeForFee(x)]));}return v;}
const surrogateCases=['"\\ud800"','{"fee":"\\udfff"}','{"tradeSide":"open\\ud800","side":"buy"}'].map((escaped,index)=>{const row=JSON.parse(escaped);return {escaped,result:capture(()=>index===0?fills.num(row):index===1?fills.totalFeeOf(row):fills.legAndDirectionOf(row))};});
const fillSurrogateCases=['symbol','tradeId'].map(field=>{const row={...fillBase,[field]:'\ud800'};return {escaped:JSON.stringify(row),resultText:JSON.stringify(mapFill(row))};});
const billSurrogateCases=['billId','businessType'].map(field=>{const row={...billBase,[field]:'\ud800'};return {escaped:JSON.stringify(row),resultText:JSON.stringify(capture(()=>income.mapBitgetBill(row)))};});
const billPrototypeBoundaries=Object.getOwnPropertyNames(Object.prototype).map(key=>{
 const row={...billBase,businessType:key};const records=income.mapBitgetBill(row);
 return {key,input:encode(row),categoryType:typeof records[0].category,
  categoryIdentity:records[0].category===(key==='__proto__'?Object.prototype:Object.prototype[key]),
  categoryPresent:Object.hasOwn(records[0],'category'),
  json:JSON.stringify(records)};
});
if(process.argv.includes('--finding-policy')) {
 const members=['constructor','__defineGetter__','__defineSetter__','hasOwnProperty','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toString','valueOf','__proto__','toLocaleString'];
 const prototype=members.map(key=>{
  const records=income.mapBitgetBills([{...billBase,businessType:key}]);
  assert.equal(records.length,1,'one non-trade inherited bill record');
  const expected=key==='__proto__'?Object.prototype:Object.prototype[key];
  assert.equal(records[0].category,expected,'source category native identity BEFORE serialization');
  assert.equal(records[0].tranId,`${billBase.billId}:${String(expected)}`,'source open-string interpolation');
  return {key,kind:typeof records[0].category,tranId:records[0].tranId};
 });
 const utf16=[{...billBase,billId:'\ud800'},{...billBase,businessType:'\udc00'}].map((input,i)=>{
  const records=income.mapBitgetBill(input);assert.ok(records.length>0);
  const value=i===0?records[0].tranId:records[0].incomeType;
  assert.equal(value.charCodeAt(0),i===0?0xd800:0xdc00,'source retained native result unit BEFORE serialization');
  return {escaped:JSON.stringify(input),field:i===0?'tranId':'incomeType',unit:value.charCodeAt(0)};
 });
 assert.equal(prototype.length,12);assert.equal(utf16.length,2);
 process.stdout.write(JSON.stringify({prototype,utf16}));process.exit(0);
}
const output=JSON.stringify({numbers,legCases,feeCases,surrogateCases,fillSurrogateCases,billSurrogateCases,fillCases,billCases,billBatches,billPrototypeBoundaries})+'\n';
const fixture=path.join(__dirname,'../tests/history-reference.json');
if(process.argv.includes('--write'))fs.writeFileSync(fixture,output);
else if(process.argv.includes('--check')){assert.equal(fs.readFileSync(fixture,'utf8'),output,'Bitget TS history source reference drift');console.log(`Bitget history reference: ${numbers.length} numeric, ${legCases.length} leg, ${fillCases.length} fill, ${billCases.length} bill, ${billBatches.length} batch cases (Rust fills and income checked)`);}
else process.stdout.write(output);
