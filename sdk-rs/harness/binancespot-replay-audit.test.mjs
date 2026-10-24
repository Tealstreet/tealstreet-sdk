import { createHmac } from 'node:crypto';
import assert from 'node:assert/strict';
import test from 'node:test';
import { captureReplayState, finalizeReplay } from './replay-finalize.mjs';
import { venueOf } from './venues.mjs';

const privateUrl = 'wss://ws-api.binance.com/ws-api/v3';
const send = (audit, frame, generation = 1) => audit.sent(privateUrl, JSON.stringify(frame), generation);
const receive = (audit, frame, generation = 1) => audit.received(privateUrl, JSON.stringify(frame), generation);

function subscribe(audit, generation = 1, { invalid = false } = {}) {
  audit.connected(privateUrl, generation);
  const timestamp = Date.now();
  const signature = createHmac('sha256', 'replay-secret')
    .update(`apiKey=replay-key&recvWindow=5000&timestamp=${timestamp}`)
    .digest('hex');
  send(
    audit,
    {
      id: 'subscribe',
      method: 'userDataStream.subscribe.signature',
      params: { apiKey: 'replay-key', recvWindow: 5000, timestamp, signature: invalid ? 'invalid' : signature },
    },
    generation,
  );
}

function acknowledge(audit, generation = 1) {
  receive(audit, { id: 'subscribe', status: 200, result: { subscriptionId: 7 } }, generation);
}

function heartbeat(audit, generation = 1, id = 'heartbeat') {
  send(audit, { id, method: 'session.subscriptions' }, generation);
  receive(audit, { id, status: 200, result: [{ subscriptionId: 7 }] }, generation);
}

function complete(audit, generation = 1) {
  acknowledge(audit, generation);
  heartbeat(audit, generation);
  receive(audit, { subscriptionId: 7, event: { e: 'outboundAccountPosition', B: [] } }, generation);
}

for (const impl of ['ts', 'rust']) {
  const createAudit = () => venueOf('binancespot').createSocketAudit(impl, { isPrivate: true, books: [] });
  const finish = (audit) => captureReplayState({ checkpoints: [] }, audit, { final: true });

  test(`${impl}: complete signed subscription, typed ACK and heartbeat are clean`, () => {
    const audit = createAudit();
    subscribe(audit);
    complete(audit);
    assert.equal(audit.ready(true), true);
    assert.deepEqual(finish(audit).protocolErrors, []);
  });

  test(`${impl}: pre-ACK checkpoint cannot fossilize final acknowledgement errors`, () => {
    const audit = createAudit();
    subscribe(audit);
    assert.equal(audit.ready(), true);
    assert.equal(audit.ready(true), false);
    const checkpoint = captureReplayState({ checkpoints: [] }, audit);
    complete(audit);
    assert.deepEqual(finish(audit).protocolErrors, []);
    assert.deepEqual(checkpoint.protocolErrors, []);
  });

  test(`${impl}: genuine invalid signature survives a healthy replacement generation`, () => {
    const audit = createAudit();
    subscribe(audit, 1, { invalid: true });
    audit.closed(privateUrl, 1);
    subscribe(audit, 2);
    complete(audit, 2);
    assert.deepEqual(finish(audit).protocolErrors, ['Invalid Binance Spot subscription signature/key/window']);
  });

  test(`${impl}: a wrong ACK ID type remains a genuine failure after the matching ACK`, () => {
    const audit = createAudit();
    subscribe(audit);
    complete(audit);
    send(audit, { id: 42, method: 'session.subscriptions' });
    receive(audit, { id: '42', status: 200, result: [{ subscriptionId: 7 }] });
    receive(audit, { id: 42, status: 200, result: [{ subscriptionId: 7 }] });
    assert.deepEqual(finish(audit).protocolErrors, ['Uncorrelated Binance Spot acknowledgement']);
  });

  test(`${impl}: private data before the subscription ACK remains a genuine failure`, () => {
    const audit = createAudit();
    subscribe(audit);
    receive(audit, { subscriptionId: 7, event: { e: 'executionReport' } });
    complete(audit);
    assert.deepEqual(finish(audit).protocolErrors, ['Binance Spot event outside acknowledged subscription/generation']);
  });

  test(`${impl}: a final unanswered request cannot be hidden by an ACK during memory settling`, async () => {
    const audit = createAudit();
    subscribe(audit);
    complete(audit);
    send(audit, { id: 'late-heartbeat', method: 'session.subscriptions' });
    const output = await finalizeReplay(
      () => finish(audit),
      async () => {
        receive(audit, { id: 'late-heartbeat', status: 200, result: [{ subscriptionId: 7 }] });
        return { memory: 0 };
      },
    );
    assert.deepEqual(output.protocolErrors, ['Binance Spot request never acknowledged']);
  });

  test(`${impl}: an incomplete live replacement cannot borrow a closed generation's ACK`, () => {
    const audit = createAudit();
    subscribe(audit);
    complete(audit);
    audit.closed(privateUrl, 1);
    subscribe(audit, 2);
    assert.equal(audit.ready(true), false);
    assert.deepEqual(finish(audit).protocolErrors, [
      'Binance Spot subscription never acknowledged',
      'Binance Spot request never acknowledged',
    ]);
  });

  test(`${impl}: closed pending requests do not poison a complete replacement`, () => {
    const audit = createAudit();
    subscribe(audit);
    complete(audit);
    send(audit, { id: 'abandoned-heartbeat', method: 'session.subscriptions' });
    audit.closed(privateUrl, 1);
    subscribe(audit, 2);
    complete(audit, 2);
    assert.deepEqual(finish(audit).protocolErrors, []);
  });
}
