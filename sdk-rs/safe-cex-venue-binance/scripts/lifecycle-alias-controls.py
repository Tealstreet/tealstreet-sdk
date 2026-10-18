#!/usr/bin/env python3
"""Disconnect each reviewed capability and require a compiled native refusal."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
logs = workspace / 'target/lifecycle-alias-controls'
logs.mkdir(parents=True, exist_ok=True)
history = 'native_history_aliases_keep_pending_http_pages_and_error_effects'
candle = 'native_candle_aliases_seed_stream_and_cleanup_without_late_publication'
life = 'native_lifecycle_aliases_publish_store_then_resolve_tick_again_at_timer_fire'
api = 'native_api_aliases_sign_and_preserve_success_and_rejection'
remote = 'native_remote_sign_alias_carries_request_and_waits_for_host_result'
faults = []


def fault(module, alias, file, scope, before, after, test):
    faults.append((f'safe_cex_venue_binance::{module}::{alias}', file, scope, before, after, test))


fault('rest_history', 'HistoryFuture', 'rest_history.rs', '',
      'let data = get(endpoint("USER_TRADES"), omit(request))?.await?;',
      'let data = get(endpoint("USER_TRADES"), omit(request))?.await?; drop(data); let data = empty();', history)
fault('rest_history', 'HistoryGet', 'exchange_classes.rs',
      'impl crate::rest_history::HistoryBindings for Context {',
      'owner.http(client, HttpMethod::Get, path, Some(args))',
      'owner.http(client, HttpMethod::Get, path, Some({drop(args); record([])}))', history)
fault('rest_history', 'PageCall', 'rest_history.rs', '',
      '("incomeType", income_type.clone()),', '("incomeType", { let _ = &income_type; string("COMMISSION") }),', history)
fault('rest_history', 'PageResolver', 'exchange_classes.rs',
      'impl crate::rest_history::HistoryBindings for Context {',
      'Method::IncomePage', 'Method::PublicTrades', history)
fault('rest_history', 'HistoryLog', 'exchange_classes.rs',
      'impl crate::rest_history::HistoryBindings for Context {',
      'owner.base.log(&msg, severity.as_str()).map_err(Rc::new)',
      '{ let _ = (&owner, msg, severity); Ok(()) }', history)
fault('rest_history', 'HistoryEmit', 'exchange_classes.rs',
      'impl crate::rest_history::HistoryBindings for Context {',
      'Ok(Rc::new(move |msg| owner.emit(msg)))',
      'Ok(Rc::new(move |msg| { let _ = (&owner, msg); Ok(()) }))', history)
fault('rest_history', 'CandleCallback', 'rest_history.rs', '',
      'callback(latest)?;', 'drop(latest);', candle)
fault('rest_history', 'CandleCleanup', 'rest_history.rs', '',
      'disposed.set(true);\n        unsubscribe()',
      'disposed.set(true);\n        let _ = &unsubscribe; Ok(())', candle)
fault('rest_history', 'CandleListen', 'exchange_classes.rs',
      'fn capture_public(&mut self) -> Result<crate::rest_history::CandleListen, Failure> {',
      '.listen_ohlcv(opts, Rc::new(move |candle| callback(decoded(candle)?)))',
      '.listen_ohlcv(opts, Rc::new(move |_| {let _ = &callback; Ok(())}))', candle)
fault('rest_history', 'CandleFetch', 'rest_history.rs', '',
      'params.write("limit", number(1.0));', 'params.write("limit", number(2.0));', candle)
fault('rest_history', 'SeedFuture', 'rest_history.rs', '',
      'b.launch(task)?;', 'drop(task);', candle)
fault('api_factory', 'ApiFuture', 'native_host.rs', '',
      'RequestReturn::Future(future) => future.await?,',
      'RequestReturn::Future(_future) => return Err(failure("disconnected ApiFuture")),', api)
fault('api_factory', 'RequestInterceptor', 'api_factory.rs', 'fn request(',
      'if public {', 'if { let _ = public; true } {', api)
fault('api_factory', 'ResponseInterceptor', 'api_factory.rs', 'fn success_response()',
      '&& data.read("code")?.number()? < 0.0', '&& data.read("code")?.number()? > 0.0', api)
fault('api_factory', 'RemoteSignCall', 'native_host.rs',
      'fn remote_sign(&self, _signer: EventValue) -> Result<RemoteSignCall, Failure> {',
      'Ok(EventValue::from_json(value))', 'drop(value); Ok(record([]))', remote)
fault('lifecycle', 'LifecycleFuture', 'lifecycle.rs', '',
      'b.capture_async(k)?(vec![])?.await',
      '{ b.capture_async(k)?(vec![])?.await?; Ok(EventValue::Undefined) }', life)
fault('lifecycle', 'LifecycleCall', 'exchange_classes.rs',
      'impl crate::lifecycle::LifecycleBindings for Context {',
      'Update => owner.update_store(arg(0))?,', 'Update => { let _ = arg(0); },', life)
fault('lifecycle', 'LifecycleAsyncCall', 'exchange_classes.rs',
      'impl crate::lifecycle::LifecycleBindings for Context {',
      'owner.invoke(method, args)', 'owner.invoke({let _ = method; Method::Orders}, args)', life)
fault('lifecycle', 'TickResolver', 'exchange_classes.rs',
      'fn tick_resolver(&mut self) -> crate::lifecycle::TickResolver {',
      'owner.invoke(Method::Tick, args)', 'owner.invoke(Method::Orders, args)', life)
fault('lifecycle', 'TickTimer', 'lifecycle.rs', 'fn schedule(',
      'tick(args)', '{ let _ = (tick, args); Ok(Box::pin(async {Ok(EventValue::Undefined)})) }', life)
assert len(faults) == 20 and len({row[0] for row in faults}) == 20
fault('exchange_classes', 'income_records', 'exchange_classes.rs', 'pub(crate) fn income_records(',
      'typed(&rows)', 'typed(&EventValue::from_json(json_value(&rows)?))',
      'native_income_range_retains_source_number_profiles')
fault('native_host', 'paginate', 'native_host.rs', 'fn paginate(',
      'safe_cex_exchange::js_runtime::value::from_typed(&result.records)\n                        .map_err(|error| exchange_failure(error.to_exchange_error()))?',
      'EventValue::from_json(serde_json::to_value(&result.records).map_err(|error| failure(error.to_string()))?)',
      'native_income_range_retains_source_number_profiles')
resume = sys.argv[1:] == ['--resume']
if len(sys.argv) > 1 and not resume:
    assert len(sys.argv) == 2
    faults = [row for row in faults if row[0].endswith('::' + sys.argv[1])]
    assert len(faults) == 1
sources = {file: (crate / 'src' / file).read_bytes() for _, file, *_ in faults}
receipt = {'sources': {file: hashlib.sha256(data).hexdigest() for file, data in sources.items()}, 'controls': []}
if resume:
    saved = json.loads((logs / 'results.json').read_text())
    assert saved['sources'] == receipt['sources'], 'resume requires exact restored production source'
    expected = {owner: 'lifecycle_aliases::' + test for owner, *_, test in faults}
    completed = {row['owner'] for row in saved['controls']}
    assert len(completed) == len(saved['controls'])
    for row in saved['controls']:
        assert row['test'] == expected[row['owner']] and row['compiledNamedRed'] and row['exactRestorationGreen']
    receipt = saved
    faults = [row for row in faults if row[0] not in completed]


def execute(test, label):
    path = logs / (label + '.log')
    name = 'lifecycle_aliases::' + test
    with path.open('w') as output:
        result = subprocess.run(['cargo', 'test', '--locked', '--color', 'never', '-p',
            'safe-cex-venue-binance', '--features', 'native', '--jobs', '1', '--test',
            'private_classes', name, '--', '--exact'], cwd=workspace,
            env={**os.environ, 'CARGO_BUILD_JOBS': '1'}, stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, path.read_text(), name


for owner, file, scope, before, after, test in faults:
    source = crate / 'src' / file
    original = sources[file]
    assert source.read_bytes() == original
    text = original.decode()
    offset = text.index(scope) if scope else 0
    index = text.index(before, offset)
    alias = owner.rsplit('::', 1)[1]
    try:
        source.write_text(text[:index] + text[index:].replace(before, after, 1))
        status, log, name = execute(test, alias + '-red')
        assert status != 0, owner + ': disconnected production capability still passed'
        assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M), owner + ': compiler error is not a control'
        assert f'test {name} ... FAILED' in log and '0 passed; 1 failed' in log, owner + ': named native assertion required'
    finally:
        source.write_bytes(original)
        assert source.read_bytes() == original
    status, log, name = execute(test, alias + '-restored')
    assert status == 0 and '1 passed; 0 failed' in log, owner + ': restored production witness failed'
    receipt['controls'].append({'owner': owner, 'test': name, 'compiledNamedRed': True, 'exactRestorationGreen': True})
    (logs / 'results.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(owner + ': compiled named RED; exact restoration GREEN', flush=True)
