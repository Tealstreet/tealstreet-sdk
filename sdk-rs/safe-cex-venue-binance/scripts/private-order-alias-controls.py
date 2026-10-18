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
logs = workspace / 'target/private-order-alias-controls'
logs.mkdir(parents=True, exist_ok=True)
cancel = 'native_cancel_capabilities_wait_for_http_before_store_status'
update = 'native_update_capabilities_cancel_then_format_and_place'
batch = 'native_batch_capabilities_classify_rejections_and_log_close_conflicts'
rate = 'native_rate_hook_defers_fourth_private_read_until_window_opens'
key = 'native_key_future_stays_pending_until_terminal_response'
retry = 'native_key_and_retry_capabilities_reconnect_and_cancel_refresh'
fields = 'native_private_handler_and_event_array_keep_numeric_fields_before_transport'
faults = []


def fault(module, alias, file, scope, before, after, test):
    faults.append((f'safe_cex_venue_binance::{module}::{alias}', file, scope, before, after, test))


fault('order_operations', 'OperationFuture', 'order_operations.rs', 'async fn delete(',
      'f(endpoint(venue, key), params()?)?.await?;',
      'drop(f(endpoint(venue, key), params()?)?);', cancel)
fault('order_operations', 'Operation', 'order_operations.rs', 'async fn update(',
      'place(new)?.await', '{ let _ = &place; drop(new); Ok(array(vec![])) }', update)
fault('order_operations', 'Delete', 'exchange_classes.rs', 'impl crate::order_operations::OrderBindings for Context {',
      'owner.http(Client::Private, HttpMethod::Delete, path, Some(args))',
      'owner.http(Client::Private, HttpMethod::Delete, path, Some({drop(args); record([])}))', cancel)
fault('order_operations', 'Sync', 'exchange_classes.rs', 'impl crate::order_operations::OrderBindings for Context {',
      'Ok(Rc::new(move |opts| owner.format_order(opts)))',
      'Ok(Rc::new(move |opts| {let _ = (&owner, opts); Ok(record([("regular", EventValue::array(vec![])), ("algo", EventValue::array(vec![]))]))}))', update)
fault('order_operations', 'Status', 'exchange_classes.rs', 'fn capture_status(',
      '.update_orders_status(&ids.iter().map(String::as_str).collect::<Vec<_>>(), status)',
      '.update_orders_status(&ids.iter().map(String::as_str).collect::<Vec<_>>(), {let _ = status; safe_cex_types::OrderStatus::Open})', cancel)
fault('order_batches', 'Post', 'exchange_classes.rs', 'fn capture_post(',
      'owner.http(Client::Unlimited, HttpMethod::Post, path, Some(args))',
      'owner.http(Client::Unlimited, HttpMethod::Post, path, Some({drop(args); record([])}))', batch)
fault('order_batches', 'Classifier', 'exchange_classes.rs', 'fn capture_post_only(',
      'crate::order_logic::is_post_only_rejection_error(Some(&value))',
      'crate::order_logic::is_post_only_rejection_error(Some({let _ = &value; &serde_json::Value::Null}))', batch)
fault('order_batches', 'Log', 'exchange_classes.rs', 'impl crate::order_batches::BatchBindings for Context {',
      'owner.base.log(&msg, severity.as_str()).map_err(Rc::new)',
      'owner.base.log(&msg, {let _ = severity; "warning"}).map_err(Rc::new)', batch)
fault('private_session', 'RateHook', 'private_session.rs', '',
      'r.rate_limit(xhr, 3.0)', 'r.rate_limit(xhr, 4.0)', rate)
fault('private_session', 'NativeKeyFuture', 'private_classes.rs', 'fn fetch_native_key(',
      'Ok(Box::pin(class_jobs::reaction(task)))',
      'Ok(Box::pin(async move {drop(task); std::future::pending().await}))', key)
fault('private_session', 'RetryAction', 'private_session.rs', 'async fn connect_legacy(',
      'again.start_connect()?;', 'let _ = &again;', retry)
fault('private_events', 'PrivateHandler', 'exchange_classes.rs', 'fn capture_store(',
      'UpdateOrder => owner\n                    .base\n                    .store\n                    .update_order(&arg(0).read("id")?.text()?, &patch(&arg(1))?)\n                    .map_err(|error| Rc::new(error.into())),',
      'UpdateOrder => Ok(()),', fields)
fault('private_event_value', 'EventArray', 'exchange_classes.rs', 'impl PrivateParent for ExchangeState {',
      'Ok(rows)', 'rows.borrow_mut().clear(); Ok(rows)', fields)
assert len(faults) == 13 and len({row[0] for row in faults}) == 13
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
    expected = {owner: 'private_order_aliases::' + test for owner, *_, test in faults}
    completed = {row['owner'] for row in saved['controls']}
    assert len(completed) == len(saved['controls'])
    for row in saved['controls']:
        assert row['test'] == expected[row['owner']] and row['compiledNamedRed'] and row['exactRestorationGreen']
    receipt = saved
    faults = [row for row in faults if row[0] not in completed]


def execute(test, label):
    path = logs / (label + '.log')
    name = 'private_order_aliases::' + test
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
