#!/usr/bin/env python3
"""Compiled semantic faults on the real native public listener graph, restored exactly."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
logs = workspace / 'target/public-listener-alias-controls'
logs.mkdir(parents=True, exist_ok=True)
name = 'public_listener_aliases::production_native_public_listener_aliases_wait_buffer_reuse_and_unsubscribe'
command = ['cargo','test','--color','never','--locked','-p','safe-cex-venue-binance',
           '--features','native','--jobs','1','--test','private_classes',name,'--','--exact']
files = {name: crate / ('src/' + name) for name in ['public_listeners.rs','native_host.rs','orderbook.rs']}
originals = {name: file.read_bytes() for name,file in files.items()}
receipt = {'sources': {name: hashlib.sha256(data).hexdigest() for name,data in originals.items()}, 'controls': []}


def execute(label):
    with (logs / (label + '.log')).open('w') as output:
        result = subprocess.run(command, cwd=workspace, env={**os.environ,'CARGO_BUILD_JOBS':'1'}, stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, (logs / (label + '.log')).read_text()


status, log = execute('baseline-green')
assert status == 0 and '1 passed; 0 failed' in log
faults = [
    ('Action','public_listeners.rs','Rc::new(move || this.wait(p.clone()))','Rc::new(move || { let _ = (&this, &p); Ok(()) })'),
    ('Parser','public_listeners.rs','let parser: Parser = Rc::new(move |events| c.parse(events));','let parser: Parser = Rc::new(move |events| { if matches!(events, EventValue::Undefined) { c.parse(events) } else { Ok(()) } });'),
    ('Book','public_listeners.rs','(this.callback)(this.book.clone())?;','(this.callback)(Rc::new(RefCell::new(this.book.borrow().clone())))?;'),
    ('Snapshot','public_listeners.rs','let data = request?.await?;','let _fulfilled = request?.await?; let data = EventValue::Undefined;'),
    ('Job','native_host.rs','if let Err(error) = job.await {','if let Err(error) = { drop(job); Ok::<(), Failure>(()) } {'),
    ('SyncState','orderbook.rs','state.last_applied_update_id = Some(final_id);','state.last_applied_update_id = Some(0.0);'),
    ('Unsubscribe','public_listeners.rs','Ok(Rc::new(move || this.unsubscribe(&parser)))','Ok(Rc::new(move || { if this.host.connected() { Ok(()) } else { this.unsubscribe(&parser) } }))'),
]
for owner, filename, before, after in faults:
    assert all(files[key].read_bytes() == data for key,data in originals.items())
    text = originals[filename].decode()
    assert text.count(before) == 1, owner
    try:
        files[filename].write_text(text.replace(before, after))
        status, log = execute(owner + '-red')
        assert status != 0, owner + ': semantic fault survived'
        assert not re.search(r'^error(?:\[|: (?!test failed\b))', log, re.M), owner + ': compiler errors do not count'
        assert f'test {name} ... FAILED' in log, owner + ': named native assertion must fail'
        assert '0 passed; 1 failed' in log
    finally:
        files[filename].write_bytes(originals[filename])
        assert files[filename].read_bytes() == originals[filename]
    status, log = execute(owner + '-exact-restoration-green')
    assert status == 0 and '1 passed; 0 failed' in log, owner
    receipt['controls'].append({'owner': 'safe_cex_venue_binance::public_listeners::' + owner, 'namedNativeFailure': True, 'exactRestorationPass': True})
    (logs / 'results.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(owner + ': named compiled native RED; exact restoration GREEN', flush=True)
assert all(files[key].read_bytes() == data for key,data in originals.items())
