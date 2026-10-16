"""Compile one production mutation per type row, require its named assertion, restore."""
from pathlib import Path
import os
import subprocess
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bitrue/src/wire.rs'
original=source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitrue','--test','wire','--jobs','1']
rows=[
 ('BITRUE_ORDER_TYPE','BitrueOrderType','#[serde(rename = "LIMIT")]','#[serde(rename = "WRONG")]'),
 ('FETCH_ACCOUNT_RESPONSE','FetchAccountResponseWire','#[serde(rename = "code")]','#[serde(rename = "wrong")]'),
 ('FETCH_CONTRACTS_RESPONSE','FetchContractsResponseItemWire','#[serde(rename = "symbol")]','#[serde(rename = "wrong")]'),
 ('FETCH_LEVERAGE_BRACKET_RESPONSE','FetchLeverageBracketResponseWire','#[serde(rename = "msg")]','#[serde(rename = "wrong")]'),
 ('FETCH_KLINE_RESPONSE','FetchKlineResponseItemWire','#[serde(rename = "high")]','#[serde(rename = "wrong")]'),
 ('ORDER_BOOK_WS_UPDATE','OrderBookWsUpdateWire','#[serde(rename = "asks")]','#[serde(rename = "buys")]'),
 ('TICKER_WS_UPDATE','TickerWsUpdateWire','#[serde(rename = "amount")]','#[serde(rename = "wrong")]'),
 ('FETCH_ORDERS_RESPONSE','FetchOrdersResponseWire','#[serde(rename = "msg")]','#[serde(rename = "wrong")]'),
 ('FETCH_MY_TRADES_RESPONSE','FetchMyTradesResponseWire','#[serde(rename = "msg")]','#[serde(rename = "wrong")]'),
 ('WS_ORDER_TRADE_UPDATE_BOOL_SWAP','WsOrderTradeUpdateOWire','#[serde(rename = "R")]','#[serde(rename = "m")]'),
 ('WS_ORDER_TRADE_UPDATE','WsOrderTradeUpdateWire','#[serde(rename = "E")]','#[serde(rename = "wrong")]'),
]
for row,record,before,after in rows:
    start=original.index(('pub enum ' if record=='BitrueOrderType' else 'struct ')+record+' {')
    end=original.index('\n}',start)
    body=original[start:end]
    assert body.count(before)==1,(record,before)
    replacement=body.replace(before,after)
    if row=='ORDER_BOOK_WS_UPDATE':
        # Swap BOTH Serde names: a serialization roundtrip still succeeds.
        replacement=body.replace('rename = "asks"','rename = "TEMP"').replace('rename = "buys"','rename = "asks"').replace('rename = "TEMP"','rename = "buys"')
    if row=='WS_ORDER_TRADE_UPDATE_BOOL_SWAP':
        replacement=body.replace('rename = "R"','rename = "TEMP"').replace('rename = "m"','rename = "R"').replace('rename = "TEMP"','rename = "m"')
    try:
        source.write_text(original[:start]+replacement+original[end:])
        result=subprocess.run(command,capture_output=True,text=True)
        output=result.stdout+result.stderr
        assertion=('native type parity: ' if row in ('ORDER_BOOK_WS_UPDATE','WS_ORDER_TRADE_UPDATE_BOOL_SWAP') else 'type shape: ')+row.removesuffix('_BOOL_SWAP')
        assert result.returncode!=0 and assertion in output,output
        print('FAIL PROVED:',assertion,flush=True)
    finally:
        source.write_text(original)
    result=subprocess.run(command,capture_output=True,text=True)
    assert result.returncode==0,result.stdout+result.stderr
    print('RESTORED PASS:',row,flush=True)
# Permanent fail-closed source-census controls, one process per mutated corpus.
census=root/'sdk-rs/safe-cex-venue-bitrue/scripts/type-schema.cjs'
text=census.read_text()
for before,after,assertion in [
    ('fields.push(...declaration(base.expression.getText(source)).fields);','/* omit inherited fields */','inherited required field census'),
    ("assert.ok(arms.every", "assert.ok(true || arms.every", 'mixed union census fails closed'),
]:
    assert text.count(before)==1
    try:
        census.write_text(text.replace(before,after))
        result=subprocess.run(['node',str(census)],capture_output=True,text=True)
        assert result.returncode!=0 and assertion in result.stderr,result.stderr
        print('FAIL PROVED:',assertion,flush=True)
    finally:
        census.write_text(text)
    result=subprocess.run(['node',str(census)],capture_output=True,text=True)
    assert result.returncode==0,result.stderr
    print('RESTORED PASS:',assertion,flush=True)
