import assert from 'node:assert/strict';

const decode = (wire) => wire ? JSON.parse(wire) : [];

// Immediate fixture routes complete the real engine handshakes before HTTP is observable.
export function releasedHttpActions(engine, wire, now) {
  const queue = decode(wire), actions = [], resources = new Map();
  let resourceId = 0n, visited = 0;
  while (queue.length) {
    assert.ok(++visited <= 65_536, 'Managed fixture drain exceeded its bound');
    const action = queue.shift();
    assert.ok(action.op !== 'sign' && action.op !== 'signPayload', 'Fixture requires its trusted local signer');
    if (action.op !== 'managedHttp') { actions.push(action); continue; }
    if (action.phase === 'prepare') {
      assert.ok(engine.httpPreparationPending(action.preparation));
      assert.ok(!resources.has(action.preparation), 'Cannot replace a prepared fixture resource');
      const resource = String(++resourceId);
      resources.set(action.preparation, { resource, request: action.packet.request });
      queue.unshift(...decode(engine.httpPrepared(action.preparation, resource, undefined, now)));
    } else if (action.phase === 'bind') {
      const prepared = resources.get(action.preparation);
      assert.ok(prepared, 'Bind needs the exact preparation');
      assert.equal(prepared.request, action.packet.request);
      const preparation = fields(action.preparation, 'p1', 3);
      const permit = fields(action.permit, 'r1', 4);
      assert.deepEqual(permit.slice(0, 2), preparation.slice(0, 2));
      assert.equal(permit[0], String(action.packet.request));
      assert.equal(permit[2], prepared.resource);
      resources.delete(action.preparation);
      const claim = JSON.parse(engine.httpClaim(action.permit, now));
      assert.ok(claim.status === 'released' || claim.status === 'denied', 'Fixture cannot hide pacing waits');
      if (claim.status === 'released') actions.push({ ...action.packet, op: 'http' });
      queue.unshift(...decode(engine.prepareDispatch(now)));
    } else if (action.phase === 'cancel') {
      resources.delete(action.preparation);
    } else assert.fail('Unexpected managed fixture phase');
  }
  assert.equal(resources.size, 0, 'Fixture cannot hide parked provider work');
  return actions;
}

function fields(token, prefix, count) {
  assert.equal(typeof token, 'string');
  const parts = token.split(':');
  assert.equal(parts.shift(), prefix);
  assert.equal(parts.length, count);
  for (const part of parts) {
    assert.match(part, /^(?:0|[1-9][0-9]*)$/);
    assert.ok(part.length < 20 || (part.length === 20 && part <= '18446744073709551615'));
  }
  return parts;
}
