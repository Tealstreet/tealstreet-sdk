"""Production field/side/invariant controls; one subprocess per entire corpus."""
from pathlib import Path
import os
import subprocess
crate=Path(__file__).resolve().parent.parent
source=crate/'src/utils.rs'
original=source.read_text()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','orderbook','--','--nocapture']
controls=[
(original.replace('price: parsed_price,','price: parsed_amount,').replace('amount: parsed_amount,','amount: parsed_price,'),'book::normalizeOrderBookSide::distinct::route_0::0::price'),
(original.replace('pick("bids", "b")','pick("__temp__", "__temp__")').replace('pick("asks", "a")','pick("bids", "b")').replace('pick("__temp__", "__temp__")','pick("asks", "a")'),'book::normalizeOrderBook::distinct::route_0::bids::0::price'),
(original.replace('safe_cex_store::orderbook::calc_order_book_total(&mut book);','safe_cex_store::orderbook::calc_order_book_total(&mut book);\n    for row in book.bids.iter_mut().chain(&mut book.asks) { std::mem::swap(&mut row.amount, &mut row.total); }'),'book::normalizeOrderBook::distinct::route_0::bids::1::amount')]
try:
 for mutated,assertion in controls:
  assert mutated!=original
  source.write_text(mutated)
  result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assert result.returncode not in (0,126) and assertion in output and 'assertion `left == right` failed' in output,output
  print('FAIL observed: '+assertion,flush=True)
  source.write_text(original)
finally:source.write_text(original)
subprocess.run(command,check=True)
print('Restored PASS after 3 order-book production swaps',flush=True)
