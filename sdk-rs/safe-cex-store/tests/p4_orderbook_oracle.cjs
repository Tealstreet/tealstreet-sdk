const path=require('node:path');const {root,load}=require('../../safe-cex-types/scripts/accounts-source.cjs');const {sortOrderBook,calcOrderBookTotal}=load(path.join(root,'packages/safe-cex/src/utils/orderbook.ts'));
let seed=734527;const random=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/2**32);const rows=[];
for(let test=0;test<200;test++){const book={asks:[],bids:[]};for(const side of ['asks','bids'])for(let i=0;i<test%61;i++)book[side].push({price:Math.floor(random()*20),amount:Math.floor(random()*20)/10,total:i});const input=JSON.parse(JSON.stringify(book));sortOrderBook(book);const sorted=JSON.parse(JSON.stringify(book));calcOrderBookTotal(book);rows.push({input,sorted,total:book});}
// The source's NaN comparator is nontransitive; pin a malformed-book permutation.
const prices=[3,NaN,2,1],book={asks:prices.map((price,i)=>({price,amount:1,total:i})),bids:[]};sortOrderBook(book);const ids=book.asks.map(x=>x.total);if(JSON.stringify(ids)!==JSON.stringify([0,1,3,2]))throw Error('source NaN comparator behavior changed');
process.stdout.write(JSON.stringify(rows));
