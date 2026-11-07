import test from 'node:test';
import assert from 'node:assert/strict';
import { venueOf } from './venues.mjs';
import { captureReplayState } from './replay-finalize.mjs';

const privateUrl = 'wss://trades.grvt.io/ws';
const streams = ['v1.order', 'v1.position', 'v1.fill'];

function authenticate(audit, generation = 1) {
  audit.connected(privateUrl, generation);
  audit.sent(privateUrl, JSON.stringify({
    type: 'auth', session_cookie: 'rm=true; gravity=fixture-session-1',
    account_id: 'fixture-account', testnet: false,
  }), generation);
  assert.equal(audit.ready(), true);
}

function complete(audit, generation = 1) {
  audit.received(privateUrl, '{"type":"auth_success"}', generation);
  for (const stream of streams) {
    audit.sent(privateUrl, JSON.stringify({
      request_id: 1, method: 'subscribe', stream, feed: ['777'], is_full: false,
    }), generation);
  }
  audit.received(privateUrl, 'ping', generation);
  audit.sent(privateUrl, 'pong', generation);
}

for (const impl of ['ts', 'rust']) {
  const create = () => venueOf('grvt').createSocketAudit(impl, { isPrivate: true, books: [] });
  test(`${impl}: complete GRVT private lifecycle has no final diagnostics`, () => {
    const audit = create(); authenticate(audit); complete(audit);
    assert.deepEqual(captureReplayState({}, audit, { final: true }).protocolErrors, []);
  });
  test(`${impl}: observing a GRVT checkpoint before ACK cannot poison healthy final coverage`, () => {
    const audit = create(); authenticate(audit);
    captureReplayState({ checkpoint: 0 }, audit);
    complete(audit);
    assert.deepEqual(captureReplayState({}, audit, { final: true }).protocolErrors, []);
  });
  test(`${impl}: a real GRVT private-data violation survives later healthy coverage`, () => {
    const audit = create(); authenticate(audit);
    audit.received(privateUrl, '{"s":"v1.fill","s1":"777-BTC_USDT_Perp","f":{"sa":"777"}}', 1);
    complete(audit);
    assert.deepEqual(captureReplayState({}, audit, { final: true }).protocolErrors, [
      'unsubscribed GRVT private data: v1.fill',
    ]);
  });
  test(`${impl}: a genuinely late GRVT selector violation remains a final failure`, () => {
    const audit = create(); authenticate(audit); complete(audit);
    captureReplayState({ checkpoint: 0 }, audit);
    audit.received(privateUrl, '{"s":"v1.fill","s1":"778-BTC_USDT_Perp","f":{"sa":"778"}}', 1);
    assert.deepEqual(captureReplayState({}, audit, { final: true }).protocolErrors, [
      'GRVT private data changed sub-account selector',
    ]);
  });
  test(`${impl}: a closed valid GRVT generation cannot cover an incomplete replacement`, () => {
    const audit = create(); authenticate(audit); complete(audit);
    audit.closed(privateUrl, 1);
    authenticate(audit, 2);
    assert.deepEqual(captureReplayState({}, audit, { final: true }).protocolErrors, streams.map(
      (stream) => `missing GRVT private stream: ${stream}`,
    ));
  });
}
