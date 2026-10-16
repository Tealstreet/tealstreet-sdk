import assert from 'node:assert/strict';
import fs from 'node:fs';
import {ts} from './source_loader.mjs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../../../packages/safe-cex/src/utils/updateOrderBook.ts',import.meta.url),'utf8');
const ast=ts.createSourceFile('book.ts',source,ts.ScriptTarget.Latest,true);
const body=ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n').replace(/\bexport /g,'');
const context=vm.createContext({});vm.runInContext(ts.transpileModule(body+'\nglobalThis.object=updateOrderBook;globalThis.primitive=updateOrderBookByPrice;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const cases=[];
for(const initial of [{bids:[],asks:[]},{bids:[{price:12,amount:1,total:99},{price:10,amount:2,total:88},{price:8,amount:3,total:77}],asks:[{price:8,amount:3,total:77},{price:10,amount:2,total:88},{price:12,amount:1,total:99}]},{bids:[{price:10,amount:1,total:5},{price:10,amount:2,total:6}],asks:[{price:10,amount:1,total:5},{price:10,amount:2,total:6}]}]) {
 const book=structuredClone(initial),steps=[];
 for(const side of ['bids','asks'])for(const object of [true,false])for(const upsert of [true,false])for(const price of [10,11,7,15,10])for(const amount of [4,0,-2,0.1]) {
  const update={price,amount,total:123};
  const index=object?context.object(book,side,update,upsert):context.primitive(book,side,price,amount,upsert);
  steps.push(structuredClone({side,object,upsert,update,index,book}));
 }
 cases.push({initial,steps});
}
// Corpus pins prevent empty or truncated snapshots from turning this gate green.
assert.equal(cases.length, 3);
for (const c of cases) assert.equal(c.steps.length, 160);
const path=new URL('./order_book_oracle.json',import.meta.url),output=JSON.stringify(cases,null,2)+'\n';
if(process.argv.includes('--check')){if(fs.readFileSync(path,'utf8')!==output)throw Error('order book source drift');}else fs.writeFileSync(path,output);
console.log(`${cases.reduce((n,c)=>n+c.steps.length,0)} order-book source frames verified`);
