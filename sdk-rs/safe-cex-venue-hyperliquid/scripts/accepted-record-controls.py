#!/usr/bin/env python3
"""Compile one record guard removal at a time, then replay all restored owners."""
import hashlib,json,os,re,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
CRATE=ROOT/'safe-cex-venue-hyperliquid'
SOURCE=CRATE/'src/types.rs'
SCHEMA=json.loads((CRATE/'tests/fixtures/accepted_flat_record_schema.json').read_text())
NAMES=('ClearinghouseStateAssetPositionsItemPositionCumFunding','ClearinghouseStateAssetPositionsItemPositionLeverage','ClearinghouseStateCrossMarginSummary','ClearinghouseStateMarginSummary','WsTrade','WsLevel','WsOrderOrder','WsUserFunding','WsLiquidation','WsNonUserCancel','L2BookLevelsItem0Item','L2BookLevelsItem1Item','FundingHistoryEntry','UserOpenOrder','CancelOrderRequest','CancelOrderRequestsItem','Signature','Notification','BuilderInfo','BuilderFeeApprovalResult')
assert set(SCHEMA)==set(NAMES),'only the exact reviewed 20-owner cohort is supported'
ORIGINAL=SOURCE.read_bytes()
ENV={**os.environ,'RUSTUP_TOOLCHAIN':'1.99.0','RUSTC_BOOTSTRAP':'1','CARGO_BUILD_JOBS':'1'}
def test_name(ident):return ident.lower()+'_native_fields_and_map_only'
def run(ident=None,failure=False):
 command=['cargo','+1.99.0','test','--locked','-p','safe-cex-venue-hyperliquid','--jobs','1','--test','accepted_flat_records']
 if ident:command += [test_name(ident),'--','--exact']
 r=subprocess.run(command,cwd=ROOT,env=ENV,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 print(r.stdout,flush=True)
 if failure:
  assert r.returncode==101 and 'could not compile' not in r.stdout and not re.search(r'error\[E\d+\]',r.stdout),'compiler failure is not a guard control'
  assert re.search(rf'^test {re.escape(test_name(ident))} \.\.\. FAILED$',r.stdout,re.M)
  assert 'complete valid positional array admitted' in r.stdout,'named native positional-array failure required'
 else:
  assert r.returncode==0
  for name in ([ident] if ident else NAMES):assert re.search(rf'^test {re.escape(test_name(name))} \.\.\. ok$',r.stdout,re.M)
try:
 run()
 for ident in NAMES:
  assert SOURCE.read_bytes()==ORIGINAL,'previous owner must be exactly restored before the next mutation'
  text=ORIGINAL.decode();anchor=f"impl<'de> Deserialize<'de> for {ident} {{"
  assert text.count(anchor)==1
  start=text.index(anchor);end=text.index('\n}',start)+2;body=text[start:end]
  assert hashlib.sha256(body.encode()).hexdigest()==SCHEMA[ident]['publicDeserializerSha256'],'review the changed concrete owner before mutation'
  assert all(kind in ('string','number','boolean') for _,_,kind in SCHEMA[ident]['fields'])
  assert 'struct OnlyMap;' in body and 'JsNumberInput::deserialize(deserializer)?' in body and 'deserialize_map(decoder, OnlyMap)' in body
  replacement=anchor+f'''
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input.decode_with(|decoder| {ident}Wire::deserialize(decoder))
            .map_err(serde::de::Error::custom)
    }}
}}'''
  print('CONTROL '+ident+': remove only this owner map guard; retain numeric capture and private schema',flush=True)
  SOURCE.write_text(text[:start]+replacement+text[end:])
  try:run(ident,failure=True)
  finally:
   SOURCE.write_bytes(ORIGINAL)
   assert SOURCE.read_bytes()==ORIGINAL
 # The exact baseline is shared by all restored owners. One complete replay
 # supplies all20 named restored passes without recompiling the same baseline20times.
 run()
finally:
 SOURCE.write_bytes(ORIGINAL)
 assert SOURCE.read_bytes()==ORIGINAL
print('20 compiled individually named array-admission failures; exact source restoration after every control; final20 restored native passes.',flush=True)
