'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
const filename = path.join(__dirname, 'reads-oracle.cjs');
const source = fs.readFileSync(filename, 'utf8');
const cut = source.indexOf('async function account(');
if (cut < 0) throw Error('actual-source helper boundary missing');
const orders = fs.readFileSync(path.join(__dirname, 'orders-corpus.cjs'), 'utf8');
const tickers = fs.readFileSync(path.join(__dirname, 'reads-tickers-supplement.cjs'), 'utf8');
const insertion = ' const cases=[];';
if (!tickers.includes(insertion)) throw Error('ticker fixture insertion missing');
const extended = tickers.replace(insertion, `add('retained-prefix',{tickers:[{...row,bid:'12.5\\ud800'}]});${insertion}`);
vm.runInNewContext(source.slice(0, cut) + orders + extended + `
(async()=>{
 const order=await ordersCase({raw:JSON.stringify({openOrders:[{order_id:'native-prefix',symbol:'PF_XBTUSD',orderType:'lmt',limitPrice:'12.5\\ud800',unfilledSize:2,filledSize:0,lastUpdateTime:'2023-11-14T22:13:20.000Z'}]})});
 const ticker=(await tickerSupplement()).futuresTickers.find(row=>row.label==='retained-prefix');
 process.stdout.write(JSON.stringify({order:order.result[0].price,ticker:ticker.result[0].bid}));
})().catch(error=>{console.error(error);process.exitCode=1});
`, {require:createRequire(filename),__dirname,Buffer,process,console},{filename});
