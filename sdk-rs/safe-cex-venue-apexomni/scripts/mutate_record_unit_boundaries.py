"""Discriminate a decoded native leaf plus all fifty observer assertions.

Observer variants corrupt the supplementary returned-unit observer. They are
verification-harness controls, not maintained TypeScript body mutations.
"""
from pathlib import Path
import json
import os
import subprocess
import time

root=Path(__file__).resolve().parents[3]
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
assert subprocess.run(['git','diff','--quiet','HEAD'],cwd=root).returncode == 0
out=root/'sdk-rs/target/apexomni-record-unit-controls'/sha
out.mkdir(parents=True,exist_ok=True)
receipt={'sha':sha,'attempts':[],'survivors':[],'invalid_controls':[],
         'scope':'one production decoder control;fifty supplemental observer-harness controls'}


def run(name,expected,marker=None,extra=None):
    command=[os.environ.get('CARGO','cargo'),'test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),
             '--locked','--jobs','2','-p','safe-cex-venue-apexomni','--test','record_units',
             'declared_record_unit_boundaries_are_native_first','--','--exact','--nocapture']
    start=time.monotonic()
    result=subprocess.run(command,cwd=root,env={**os.environ,**(extra or {})},capture_output=True,text=True)
    text=result.stdout+result.stderr
    (out/(name+'.log')).write_text(text)
    valid=result.returncode == expected and (('test result: FAILED.' in text and marker in text)
                                             if expected == 101 else '1 passed' in text)
    receipt['attempts'].append({'name':name,'exit':result.returncode,'expected_exit':expected,
                               'assertion':marker,'valid':valid,'seconds':round(time.monotonic()-start,2)})
    if expected == 101 and result.returncode == 0:
        receipt['survivors'].append({'control':name,'call_site':'record_units selected decoded leaf/observer'})
    elif not valid:
        receipt['invalid_controls'].append(name)
    (out/'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(name,result.returncode,'valid',valid,flush=True)
    return valid


path=root/'sdk-rs/safe-cex-venue-apexomni/src/public_records.rs'
original=path.read_bytes();source=original.decode()
before='public_key: fields.public_key,'
assert source.count(before)==1
try:
    path.write_text(source.replace(before,'public_key: format!("altered{}", fields.public_key),'))
    run('native-KeyPair-leaf',101,'APEX-RECORD-UNIT KeyPair/pair decoded native declared leaf units')
finally:
    path.write_bytes(original)
assert run('native-KeyPair-restored',0)
names=['KeyPair', 'ISO8601', 'ApiKeyCredentials', 'WithPrice', 'OrderWithClientId', 'UserObject', 'WalletObject', 'OpenPositionObject', 'AccountsItem', 'AccountObject', 'OrderObject', 'WorstPriceObject', 'CreateOrderOptionsObject', 'FundingRateObject', 'HistoricalPNLObject', 'OrderResponseObject', 'AccountBalanceObject', 'TickerObject', 'DepthObject', 'GetHistoryFundingOptions', 'PerpetualCurrencyObject', 'CurrencyObject', 'GlobalObject', 'PerpetualContractObject', 'MultiChainObject', 'PerpetualObject', 'SymbolObject', 'TradeObject', 'KlineObject', 'HistoryFundingObject', 'SymbolInfoObject', 'ChangePubKeyECDSA', 'ChangePubKeyCREATE2', 'SignChangePubKeyPayload', 'TxChangePubKeyData', 'SignedChangePubKeyData', 'SignContractPayload', 'TxContractData', 'SignedContractData', 'SignOrderPayload', 'TxOrderData', 'SignedOrderData', 'SignTransferPayload', 'TxTransferData', 'SignedTransferData', 'SignWithdrawPayload', 'TxWithdrawData', 'SignedWithdrawData', 'TxEthSignature', 'TxSignature']
assert len(names)==50
for name in names:
    run(name+'-observer-substitution',101,'APEX-RECORD-UNIT '+name+'/high source returned declared leaf units',
        {'SAFE_CEX_APEX_RECORD_OBSERVER_CONTROL':name})
    assert run(name+'-observer-restored',0),name
assert len(receipt['attempts'])==102,receipt
assert not receipt['survivors']and not receipt['invalid_controls'],receipt
assert subprocess.run(['git','diff','--quiet','HEAD'],cwd=root).returncode == 0
