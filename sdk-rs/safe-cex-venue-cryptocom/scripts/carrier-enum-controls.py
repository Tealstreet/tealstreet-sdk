"""Prove direct finite-union observations discriminate native identity/refusal."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
# Exact reviewed first branches, not regex over arbitrary adjacent implementation.
CONTROLS = [
 ('ws_payloads.rs','CryptocomWsValuationChannel','return Ok(Self::Mark);','return Ok(Self::Index);'),
 ('ws_payloads.rs','CryptocomWsSide','return Ok(Self::Buy);','return Ok(Self::Sell);'),
 ('ws_payloads.rs','CryptocomWsRefPriceType','return Ok(Self::MarkPrice);','return Ok(Self::IndexPrice);'),
 ('ws_payloads.rs','CryptocomWsPositionType','return Ok(Self::PerpetualSwap);','return Ok(Self::LongPosition);'),
 ('ws_payloads.rs','CryptocomWsIsolationType','return Ok(Self::IsolatedMargin);','return Err(PayloadError("string literal"));'),
]

def run(test):
 r=subprocess.run(['cargo','test','--locked','--jobs','1','-q','-p','safe-cex-venue-cryptocom','--test','accepted_carrier_enum_shapes',test,'--','--exact'],cwd=ROOT,capture_output=True,text=True)
 return r.returncode,r.stdout+r.stderr

def passing(test):
 code,output=run(test)
 assert code==0 and 'test result: ok. 1 passed' in output,output

for file,owner,before,after in CONTROLS:
 source=ROOT/'safe-cex-venue-cryptocom/src'/file
 test='carrier_enum_'+owner.removeprefix('Cryptocom').lower()
 passing(test)
 original=source.read_bytes()
 start=original.index(f'impl {owner} {{'.encode())
 end=original.index(b'\n}\n',start)+3
 region=original[start:end]
 assert region.count(before.encode())==1,owner
 try:
  source.write_bytes(original[:start]+region.replace(before.encode(),after.encode())+original[end:])
  code,output=run(test)
  assert code==101 and 'test result: FAILED' in output and f'---- {test} stdout ----' in output,output
  assert f'native {owner} identity/refusal' in output,output
 finally:
  source.write_bytes(original)
 assert source.read_bytes()==original
 passing(test)
 print(f'{owner}: literal identity/refusal mutation compiled and failed101; exact restoration PASS',flush=True)
