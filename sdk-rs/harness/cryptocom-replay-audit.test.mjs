import { createHmac } from 'node:crypto';
import assert from 'node:assert/strict';
import test from 'node:test';
import { captureReplayState, finalizeReplay } from './replay-finalize.mjs';
import { venueOf } from './venues.mjs';

const privateUrl = 'wss://stream.crypto.com/exchange/v1/user';
const commonChannels = ['user.order', 'user.advance.order', 'user.positions', 'user.balance'];
const requiredChannels = (impl) => [...commonChannels, ...(impl === 'rust' ? ['user.trade'] : [])];

function authenticate(audit, { invalid = false } = {}) {
  const frame = { id: 1, method: 'public/auth', api_key: 'replay-key', nonce: Date.now() };
  frame.sig = createHmac('sha256', 'replay-secret')
    .update(frame.method + frame.id + frame.api_key + frame.nonce)
    .digest('hex');
  if (invalid) frame.sig = 'invalid';
  audit.sent(privateUrl, JSON.stringify(frame));
}

function acknowledge(audit) {
  audit.received(privateUrl, JSON.stringify({ id: 1, method: 'public/auth', code: 0 }));
}

function subscribe(audit, channels) {
  audit.sent(privateUrl, JSON.stringify({ method: 'subscribe', params: { channels } }));
}

function heartbeat(audit, id, { respond = true } = {}) {
  audit.received(privateUrl, JSON.stringify({ id, method: 'public/heartbeat' }));
  if (respond) respondHeartbeat(audit, id);
}

function respondHeartbeat(audit, id) {
  audit.sent(privateUrl, JSON.stringify({ id, method: 'public/respond-heartbeat' }));
}

for (const impl of ['ts', 'rust']) {
  const createAudit = () => venueOf('cryptocom').createSocketAudit(impl, { isPrivate: true, books: [] });
  const finish = (audit) => captureReplayState({ checkpoints: [] }, audit, { final: true });

  test(`${impl}: complete authenticated subscriptions and heartbeat are clean`, () => {
    const audit = createAudit();
    authenticate(audit);
    acknowledge(audit);
    subscribe(audit, requiredChannels(impl));
    heartbeat(audit, 44);
    assert.deepEqual(finish(audit).protocolErrors, []);
  });

  test(`${impl}: pre-ACK checkpoint does not fossilize missing subscription errors`, () => {
    const audit = createAudit();
    authenticate(audit);
    assert.equal(audit.ready(), true);
    const checkpoint = captureReplayState({ checkpoints: [] }, audit);
    acknowledge(audit);
    subscribe(audit, requiredChannels(impl));
    heartbeat(audit, 44);
    assert.deepEqual(finish(audit).protocolErrors, []);
    assert.deepEqual(checkpoint.protocolErrors, []);
  });

  test(`${impl}: genuine invalid authentication survives a later valid lifecycle`, () => {
    const audit = createAudit();
    authenticate(audit, { invalid: true });
    authenticate(audit);
    acknowledge(audit);
    subscribe(audit, requiredChannels(impl));
    assert.deepEqual(finish(audit).protocolErrors, ['invalid private WS auth signature']);
  });

  test(`${impl}: genuine pre-ACK subscriptions survive a later valid lifecycle`, () => {
    const audit = createAudit();
    authenticate(audit);
    subscribe(audit, requiredChannels(impl));
    acknowledge(audit);
    subscribe(audit, requiredChannels(impl));
    assert.deepEqual(finish(audit).protocolErrors, ['private subscriptions sent before authentication']);
  });

  test(`${impl}: a genuine late heartbeat failure is captured before memory settling`, async () => {
    const audit = createAudit();
    authenticate(audit);
    acknowledge(audit);
    subscribe(audit, requiredChannels(impl));
    heartbeat(audit, 45, { respond: false });
    const output = await finalizeReplay(
      () => finish(audit),
      async () => {
        respondHeartbeat(audit, 45);
        return { memory: 0 };
      },
    );
    assert.deepEqual(output.protocolErrors, ['heartbeat response missing']);
  });

  test(`${impl}: the Rust-only user.trade requirement remains strict`, () => {
    const audit = createAudit();
    authenticate(audit);
    acknowledge(audit);
    subscribe(audit, commonChannels);
    assert.deepEqual(finish(audit).protocolErrors, impl === 'rust' ? ['missing private subscription user.trade'] : []);
  });
}
