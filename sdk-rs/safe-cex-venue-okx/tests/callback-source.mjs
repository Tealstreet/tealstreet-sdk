import assert from 'node:assert/strict';
import {loadSource} from './source-loader.mjs';
const base = '../../../packages/safe-cex/src/exchanges/okx/';
const {mapOkxHistoricalOrder} = loadSource(new URL(base+'okx.order-history.ts', import.meta.url));
const {mapOkxClosedPosition} = loadSource(new URL(base+'okx.closed-pnl.ts', import.meta.url));
const {mapOkxBillToIncomeRecords} = loadSource(new URL(base+'okx.income.ts', import.meta.url));
const {mapOkxPositionHistory} = loadSource(new URL('../../../packages/safe-cex/src/journal/okx.position-adapter.ts', import.meta.url));
const row = {instId:'BTC-USDT-SWAP',cTime:'1700000000000',uTime:'1700000001000',closeTotalPos:'2',openMaxPos:'3',closeAvgPx:'60000',openAvgPx:'59000',realizedPnl:'17',fee:'-1',fundingFee:'2',lever:'5',posId:'position-1',ccy:'',direction:'long'};
const market = {symbol:'BTCUSDT'};
let checked = 0;
function check(name, invoke) {
  const original = {message:'callback exploded',marker:'opaque cause'};
  let calls = 0;
  const callback = () => {calls++; throw original;};
  assert.throws(() => invoke(callback), error => error === original, name+' original throw identity');
  assert.equal(calls, 1, name+' stops at first failure');
  checked++;
}
check('history', callback => mapOkxHistoricalOrder({ordId:'history-1',instId:row.instId,side:'buy',posSide:'net',ordType:'limit',state:'filled',sz:'3',accFillSz:'2',px:'60011',avgPx:'60000',cTime:row.cTime,uTime:row.uTime,reduceOnly:'false'},market,callback));
check('closed', callback => mapOkxClosedPosition(row,market,callback,'account'));
check('income', callback => mapOkxBillToIncomeRecords({ts:row.cTime,instId:row.instId,balChg:'17',fee:'-1',pnl:'12',billId:'bill-1',type:'2',ccy:'USDT'},callback));
for (const slot of ['toBase','resolveSymbol','resolveSettleCurrency']) {
  check('position/'+slot, callback => mapOkxPositionHistory(row,{account:'account',toBase:()=>2,resolveSymbol:()=> 'BTCUSDT',resolveSettleCurrency:()=> 'USDT',[slot]:callback}));
}
assert.equal(checked,6);
process.stdout.write(String(checked));
