const {test} = require('node:test');
const assert = require('node:assert/strict');
const {validate, fingerprint, requireNoFindings} = require('./cancellable-futures.cjs');
const sources = {'src/guard.rs': 'impl Drop for Guard { fn drop(&mut self) { release(); } }'};
const discovery = [{owner:'src/fixture.rs::schedule', boundaries:['async-block'], producer_tokens:'let guard = pre_pin(); spawn(async move { guard.finish(); });'}];
const inventory = () => ({version:1, owners:[{owner:discovery[0].owner, boundaries:discovery[0].boundaries, producer_sha256:fingerprint(discovery[0].producer_tokens), classification:'guarded', source_dependencies:[{file:'src/guard.rs',sha256:fingerprint(sources['src/guard.rs'])}], reason:'Pre-spawn cleanup owns the reserved slot.', witnesses:['tests/drop.rs::unpolled_task_drop']}]});
test('reviewed producer baseline is accepted', () => assert.equal(validate(discovery, inventory(),sources), 1));
test('new production future fails closed', () => assert.throws(() => validate([...discovery,{...discovery[0],owner:'src/fixture.rs::new_future'}],inventory(),sources), /uncovered production future; route to core/));
test('moving pre-pin guard inside future requires fresh review', () => assert.throws(() => validate([{...discovery[0],producer_tokens:'spawn(async move { let guard = pre_pin(); guard.finish(); });'}],inventory(),sources), /future creator changed; cancellation review required/));
test('deleted future cannot leave a stale approved row', () => assert.throws(() => validate([],inventory(),sources), /stale cancellation classification/));
test('removing a nested future boundary fails', () => assert.throws(() => validate([{...discovery[0],boundaries:['future-return']}],inventory(),sources), /future boundary inventory changed/));
test('duplicate classifications fail rather than hiding omissions', () => {const rows=inventory();rows.owners.push(rows.owners[0]);assert.throws(()=>validate(discovery,rows,sources),/duplicate reviewed future owner/);});
test('unknown classification and blank rationale fail', () => {const rows=inventory();rows.owners[0].classification='safe';assert.throws(()=>validate(discovery,rows,sources),/unclassified cancellation owner/);rows.owners[0].classification='guarded';rows.owners[0].reason=' ';assert.throws(()=>validate(discovery,rows,sources),/blank cancellation reason/);});
test('guard name alone cannot substitute for task-drop witness', () => {const rows=inventory();rows.owners[0].witnesses=[];assert.throws(()=>validate(discovery,rows,sources),/needs TASK-drop witnesses/);});

test('removing Drop implementation requires review even when creator stays unchanged',()=>assert.throws(()=>validate(discovery,inventory(),{'src/guard.rs':'impl Guard { fn finish(&self) {} }'}),/guard implementation changed/));
test('shared waiter registration cannot be waved through as no-owned-latch',()=>{const rows=inventory();rows.owners[0].boundaries=['shared-waiter-registration'];rows.owners[0].classification='no-owned-latch';assert.throws(()=>validate([{...discovery[0],boundaries:rows.owners[0].boundaries}],rows,sources),/shared waiter registration needs cleanup/);});

test('routed known FINDINGS still make the gate fail',()=>{const rows=inventory();rows.owners[0].classification='finding';rows.owners[0].routed_to='core';assert.equal(validate(discovery,rows,sources),1);assert.throws(()=>requireNoFindings(rows),/OPEN production cancellation FINDINGS must be repaired/);});
