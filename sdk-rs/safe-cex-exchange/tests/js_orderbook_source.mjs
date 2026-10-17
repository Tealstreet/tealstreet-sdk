import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
// Execute the maintained utility modules, with no substitute implementation.
function load(relative) {
 const filename=fileURLToPath(new URL('../../../packages/safe-cex/src/utils/'+relative,import.meta.url));
 const module={exports:{}};
 const code=ts.transpileModule(fs.readFileSync(filename,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 vm.runInThisContext('(function(require,module,exports){'+code+'\n})',{filename})(createRequire(filename),module,module.exports);
 return module.exports;
}
const {updateOrderBook}=load('updateOrderBook.ts');
const {calcOrderBookTotal}=load('orderbook.ts');
const value=x=>x==='$undefined'?undefined:x==='$nan'?NaN:x;
const specs=[];
for(const side of ['bids','asks'])for(const price of [200,'200',150,'150',null,'$undefined','$nan'])for(const amount of [0,'0',3,null,'$undefined'])for(const upsert of [true,false])
 specs.push({side,price,amount,upsert,prices:side==='bids'?[300,200,100]:[100,200,300]});
for(const prices of [[null,200,100],['$hole',200,100],[300,null,100],[300,'$hole',100],['300','200','100']])specs.push({side:'bids',price:200,amount:3,upsert:true,prices});
specs.push({side:'bids',price:200,amount:3,secondAmount:5,upsert:true,prices:[300,200,100]});
specs.push({side:'bids',price:150,amount:3,throwAmount:true,upsert:false,prices:[300,200,100]});
const updates=specs.map((spec,index)=>{
 const log=[];const levels=spec.prices.map((price,i)=>price===null?null:price==='$hole'?undefined:Object.defineProperty({amount:1,total:0},'price',{enumerable:true,get(){log.push('level:'+i);return value(price)}}));
 spec.prices.forEach((p,i)=>{if(p==='$hole')delete levels[i]});
 let amountReads=0;
 const update=Object.defineProperties({total:0},{price:{enumerable:true,get(){log.push('price');return value(spec.price)}},amount:{enumerable:true,get(){log.push('amount');if(spec.throwAmount)throw new RangeError('amount');return value(++amountReads===2&&'secondAmount'in spec?spec.secondAmount:spec.amount)}}});
 const book={[spec.side]:levels};let result=null,error=null;
 try{result=String(updateOrderBook(book,spec.side,update,spec.upsert))}catch(e){error=e.name}
 const trace=[...log];const inserted=levels.includes(update);
 return {index,spec,result,error,trace,inserted,rows:JSON.stringify(levels)};
});
const totals=[
 {bids:[1,2],asks:[3,4]}, {bids:['1',2],asks:[null,3]},
 {bids:'$missing',asks:[]},{bids:[],asks:'$missing'},
 {bids:null,asks:[]},{bids:[],asks:null},
 {bids:[1,'$hole'],asks:[]},{bids:[1],asks:[null]},
 {bids:[1],asks:[],replaceRow:true}, {bids:[1],asks:[],appendRow:true},
].map((spec,index)=>{
 const log=[];const book={};
 for(const side of ['bids','asks']){
  if(spec[side]==='$missing')continue;
  const levels=spec[side]===null?null:spec[side].map((amount,i)=>{
   if(amount==='$hole')return undefined;
   const level={total:0};Object.defineProperty(level,'amount',{enumerable:true,get(){log.push(side+':'+i);if(side==='bids'&&i===0){if(spec.replaceRow)levels[0]={amount:9,total:0};if(spec.appendRow&&levels.length===1)levels.push({amount:2,total:0})}return value(amount)}});return level;
  });
  if(levels)spec[side].forEach((v,i)=>{if(v==='$hole')delete levels[i]});
  Object.defineProperty(book,side,{enumerable:true,get(){log.push(side);return levels}});
 }
 let error=null;try{calcOrderBookTotal(book)}catch(e){error=e.name}
 const trace=[...log];return {index,spec,error,trace,book:JSON.stringify(book)};
});
process.stdout.write(JSON.stringify({updates,totals}));
