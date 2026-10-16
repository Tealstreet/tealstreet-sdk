const assert = require('node:assert/strict');

const first = {id:'first-id',symbol:'first-symbol',price:11,size:23,buy:true,reduceOnly:false,nested:{left:'left-first',right:'right-first'},tags:['first-a','first-b']};
const second = {id:'second-id',symbol:'second-symbol',price:37,size:41,buy:false,reduceOnly:true,nested:{left:'left-second',right:'right-second'},tags:['second-a','second-b']};
assert.notDeepEqual(first,second,'collection identity distinct records');
for(const record of [first,second]) {
 assert.notEqual(record.id,record.symbol,'collection identity distinct strings');
 assert.notEqual(record.price,record.size,'collection identity distinct numbers');
 assert.notEqual(record.buy,record.reduceOnly,'collection identity distinct booleans');
 assert.notEqual(record.nested.left,record.nested.right,'collection identity distinct nested strings');
}
const book = {bids:[[11,23],[17,29]],asks:[[37,41],[43,47]]};
assert.notDeepEqual(book.bids,book.asks,'collection identity distinct sides');
const publicInputs = [
 {bids:[],asks:[]},
 {bids:book.bids,asks:book.asks},
 {order_book:book,data:{order_book:{bids:book.asks,asks:book.bids}}},
 {data:{orderbook:book},result:{orderbook:{bids:book.asks,asks:book.bids}}},
 {tickers:[first]},
 {tickers:[first,first,second,first]},
 {data:{tickers:{z:second,'10':first,'2':second,a:first}}},
 {order_book_stats:[first,second],data:{tickers:[second,first]}},
];
const privateInputs = [[],[[]],[[first]],[[first,first,second,first]],{z:[first],'10':[second],'2':[first],a:[second]}, {first,second}, {z:[second],a:first}, [[second],[first]]];
module.exports = {publicInputs,privateInputs,records:[first,second]};
