"""Captured native map guard and consumed field controls, restored byte for byte."""
import json
from pathlib import Path
import subprocess
ROOT=Path(__file__).resolve().parents[2]
SOURCE=ROOT/'safe-cex-venue-cryptocom/src/ws_payloads.rs'
SCHEMAS={'TickerData': [['i', 's'], ['b', 's'], ['k', 's'], ['a', 's'], ['c', 's'], ['h', 's'], ['l', 's'], ['v', 's'], ['vv', 's'], ['oi', '?s'], ['t', 'n']], 'ValuationData': [['v', 's'], ['t', 'n']], 'TradeData': [['d', 'n'], ['t', 'n'], ['p', 'n'], ['q', 'n'], ['s', 'eSide'], ['i', 's'], ['dataTime', 'n']], 'BookData': [['bids', 'tuples'], ['asks', 'tuples'], ['t', 'n'], ['tt', 'n']], 'CandlestickData': [['t', 'n'], ['o', 's'], ['h', 's'], ['l', 's'], ['c', 's'], ['v', 's']], 'OrderData': [['account_id', '?s'], ['order_id', 's'], ['client_oid', '?s'], ['order_type', 's'], ['time_in_force', 's'], ['side', 'eSide'], ['exec_inst', '?strings'], ['quantity', 's'], ['limit_price', '?s'], ['trigger_price', '?s'], ['ref_price', '?s'], ['ref_price_type', '?eRefPriceType'], ['avg_price', '?s'], ['cumulative_quantity', '?s'], ['cumulative_value', '?s'], ['fee_currency', '?s'], ['fee_instrument_name', '?s'], ['create_time', '?n'], ['update_time', '?n'], ['time_created', '?n'], ['time_updated', '?n'], ['status', '?s'], ['order_status', '?s'], ['instrument_name', 's'], ['reason', '?n'], ['update_user_id', '?s']], 'PositionData': [['account_id', '?s'], ['quantity', 's'], ['cost', 's'], ['open_position_pnl', 's'], ['session_pnl', '?s'], ['instrument_name', 's'], ['open_pos_cost', '?s'], ['update_timestamp_ms', '?n'], ['type', 'ePositionType'], ['mark_price', '?s'], ['target_leverage', '?s'], ['leverage', '?s'], ['liquidation_price', '?s'], ['isolated_margin_balance', '?s'], ['isolation_id', '?s'], ['isolation_type', '?eIsolationType']], 'PositionBalance': [['instrument_name', 's'], ['quantity', 's'], ['reserved_qty', 's'], ['collateral_amount', 's'], ['collateral_eligible', 'b'], ['haircut', 's'], ['market_value', 's'], ['max_withdrawal_balance', 's'], ['hourly_interest_rate', 's']], 'BalanceSummary': [['total_available_balance', 's'], ['total_margin_balance', 's'], ['total_initial_margin', 's'], ['total_maintenance_margin', 's'], ['total_cash_balance', 's'], ['total_collateral_value', 's'], ['total_session_unrealized_pnl', 's'], ['total_session_realized_pnl', 's'], ['total_position_cost', 's'], ['instrument_name', 's'], ['position_balances', 'aPositionBalance']], 'TickerUpdate': [['instrument_name', 's'], ['subscription', 's'], ['channel', 's'], ['data', 'aTickerData']], 'ValuationUpdate': [['instrument_name', 's'], ['subscription', 's'], ['channel', 'eValuationChannel'], ['data', 'aValuationData']], 'TradeUpdate': [['instrument_name', 's'], ['subscription', 's'], ['channel', 's'], ['data', 'aTradeData']], 'BookUpdate': [['instrument_name', 's'], ['subscription', 's'], ['channel', 's'], ['data', 'aBookData']], 'CandlestickUpdate': [['instrument_name', 's'], ['subscription', 's'], ['channel', 's'], ['data', 'aCandlestickData']], 'OrderUpdate': [['subscription', 's'], ['channel', 's'], ['data', 'aOrderData']], 'PositionUpdate': [['subscription', 's'], ['channel', 's'], ['data', 'aPositionData']], 'BalanceUpdate': [['subscription', 's'], ['channel', 's'], ['data', 'aBalanceSummary']]}
def run(test):
 r=subprocess.run(['cargo','test','--locked','--jobs','1','-q','-p','safe-cex-venue-cryptocom','--test','accepted_ws_record_shapes',test,'--','--exact'],cwd=ROOT,capture_output=True,text=True)
 return r.returncode,r.stdout+r.stderr
def passing(test):
 code,output=run(test);assert code==0 and 'test result: ok. 1 passed' in output,output
def control(test,original,mutant,message):
 passing(test)
 try:
  SOURCE.write_bytes(mutant)
  code,output=run(test)
  assert code==101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output,output
  assert message in output,output
 finally:SOURCE.write_bytes(original)
 assert SOURCE.read_bytes()==original
 passing(test)
for name,fields in SCHEMAS.items():
 owner='CryptocomWs'+name
 original=SOURCE.read_bytes()
 start=original.index(b"fn field<'a>");end=original.index(b'\n}\n',start)+3;region=original[start:end]
 assert region.count(b'        _ => Err(PayloadError("object")),')==1
 indexes=', '.join(json.dumps(key)+' => '+str(i) for i,(key,kind) in enumerate(fields))
 array_arm=('        PayloadValue::Array(values) => { let index=match key { '+indexes+', _ => return Err(PayloadError("unknown test position")) }; Ok(values.get(index).unwrap_or(&PayloadValue::Undefined)) },\n').encode()
 mutant=original[:start]+region.replace(b'        _ => Err(PayloadError("object")),',array_arm+b'        _ => Err(PayloadError("object")),')+original[end:]
 test='complete_array_ws_'+name.lower()
 control(test,original,mutant,'complete captured positional array accepted for '+owner)
 print(owner+': compiled array-admission assertion failure (exit 101); exact restoration PASS',flush=True)
 start=original.index(('impl '+owner+' {').encode());end=original.index(b'\n}\n',start)+3;region=original[start:end]
 selected=next(((k,t) for k,t in fields if t=='s'),None) or next((k,t) for k,t in fields if t=='n')
 key,kind=selected;decoder='string' if kind=='s' else 'number'
 before=('('+decoder+')(field(value, '+json.dumps(key)+')?)?').encode();assert region.count(before)==1
 after=(b'('+before+b').into_iter().chain([0x41u16]).collect()' if kind=='s' else b'('+before+b') + 1.0')
 mutant=original[:start]+region.replace(before,after)+original[end:]
 test='ws_record_'+name.lower()
 control(test,original,mutant,owner+' native fields')
 print(owner+': consumed '+key+' corruption failed native fields (exit 101); exact restoration PASS',flush=True)
