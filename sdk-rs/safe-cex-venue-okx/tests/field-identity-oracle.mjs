// Separate supplement: does not rewrite or retroactively certify existing corpora.
import {loadSource} from './source-loader.mjs';
const base='../../../packages/safe-cex/src/exchanges/okx/';
const history=loadSource(new URL(base+'okx.order-history.ts',import.meta.url));
const closed=loadSource(new URL(base+'okx.closed-pnl.ts',import.meta.url));
const income=loadSource(new URL(base+'okx.income.ts',import.meta.url));
const market={id:'DOGE-USDT-SWAP',symbol:'DOGEUSDT',base:'DOGE',quote:'USDT',active:true,precision:{amount:0.001,price:0.00002},limits:{amount:{min:0.003,max:97},leverage:{min:2,max:83}},contractSize:1000,isInverse:false};
const convert=(n,m,p)=>n*m.contractSize;
const seeds={
 history:{ordId:'ORDER-1',instId:'DOGE-USDT-SWAP',cTime:'1735689600000',uTime:'1735689660123',side:'buy',ordType:'market',state:'canceled',px:'0.081',avgPx:'0.072',sz:'0.14',accFillSz:'0.09',reduceOnly:'true',posSide:'short'},
 closed:{cTime:'1735689600000',uTime:'1735689660123',instId:'DOGE-USDT-SWAP',closeAvgPx:'0.071',closeTotalPos:'0.14',openAvgPx:'0.082',realizedPnl:'-0.011',direction:'short',fee:'-0.017',lever:'53'},
 income:{ts:'1735689600000',billId:'BILL-1',tradeId:'TRADE-2',instId:'DOGE-USDT-SWAP',ccy:'USDC',type:'2',balChg:'1.23',fee:'-0.017',pnl:'0.91'}
};
const cases=[];
for(const [op,row] of Object.entries(seeds)){
 const expected=op==='history'?history.mapOkxHistoricalOrder(row,market,convert):op==='closed'?closed.mapOkxClosedPosition(row,market,convert,'IDENTITY-ACCOUNT'):income.mapOkxBillToIncomeRecords(row,()=>market.symbol);
 cases.push({op,row,market,expected});
}
const journal=loadSource(new URL('../../../packages/safe-cex/src/journal/okx.position-adapter.ts',import.meta.url));
const journalRow={instId:'DOGE-USDC-SWAP',direction:'short',uTime:'1735689660123',cTime:'1735689600000',closeTotalPos:'0.42',openMaxPos:'0.28',fee:'-0.017',fundingFee:'0.003',realizedPnl:'0.91',posSide:'net',posId:'POSITION-3',ccy:'USDC',lever:'53',openAvgPx:'0.081',closeAvgPx:'0.072',type:'1'};
cases.push({op:'journal',row:journalRow,market,expected:journal.mapOkxPositionHistory(journalRow,{account:'IDENTITY-ACCOUNT',toBase:n=>n*1000,resolveSymbol:()=> 'DOGEUSDC',resolveSettleCurrency:()=> 'USDC'})});
process.stdout.write(JSON.stringify(cases));
