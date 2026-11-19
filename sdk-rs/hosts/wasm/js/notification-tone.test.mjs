import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import test from 'node:test';

import {
  CommandDeadlineError, CommandProtocolError, CommandTimeoutError,
  EngineDeadError, EngineDisposedError, ExchangeError,
} from './host.mjs';

const guidance = 'Check your open orders before trying again.';
const cause = new Error('retained diagnostic');
const makeErrors = (command) => [
  new CommandTimeoutError(120_000, command),
  new CommandDeadlineError(cause, command),
  new CommandProtocolError(cause, command),
  new EngineDisposedError(command, true),
  new EngineDeadError(cause, command),
];

// Rule 5 (Sam, 2026-11-20): calmer text keeps the interrupted-write safety contract.
test('interrupted writes keep identity and Unknown, with one calm check-before-retry action', () => {
  for (const error of makeErrors({ op: 'placeOrders', clientOrderIds: ['caller-1'] })) {
    assert.equal(error.outcome, 'unknown');
    assert.equal(error.command, 'placeOrders');
    assert.deepEqual(error.clientOrderIds, ['caller-1']);
    assert.equal(error.message.split(guidance).length - 1, 1);
    assert.doesNotMatch(error.message, /engine|outcome|receipt|ledger|admission|reconcile/i);
  }
});

test('interrupted reads do not acquire write uncertainty or order guidance', () => {
  for (const error of makeErrors({ op: 'getAccount' })) {
    assert.equal(error.outcome, undefined);
    assert.ok(!error.message.includes(guidance));
  }
  const refused = new ExchangeError('The exchange declined this order', {
    kind: 'rejected', outcome: 'notSent', venueCode: '10001',
  });
  assert.equal(refused.kind, 'rejected');
  assert.equal(refused.outcome, 'notSent');
  assert.equal(refused.venueCode, '10001');
});

test('the actual Rust capacity message stays separate from venue rate limits', async () => {
  const rust = readFileSync(new URL('../../../engine/src/error.rs', import.meta.url), 'utf8');
  const message = /Error::Capacity => "([^"\n]+)"/.exec(rust)?.[1];
  assert.ok(message);
  assert.doesNotMatch(message, /engine|admission/i);
  const source = readFileSync(new URL('../../../../packages/safe-cex/src/health/errorPatterns.ts', import.meta.url), 'utf8');
  const url = `data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(source)).toString('base64')}`;
  const { DEFAULT_ERROR_PATTERNS } = await import(url);
  const match = (text) => DEFAULT_ERROR_PATTERNS.find((row) => row.patterns.some((pattern) =>
    typeof pattern === 'string' ? text.toLowerCase().includes(pattern.toLowerCase()) : pattern.test(text)))?.category;
  assert.equal(match(message), undefined);
  assert.equal(match('Engine request capacity reached'), undefined);
  assert.equal(match('Too many requests in flight'), 'rate_limit');
  const engine = readFileSync(new URL('../../../engine/src/engine.rs', import.meta.url), 'utf8');
  const liveDataMessage = /PUBLIC_FEED_UNAVAILABLE_TEXT: &str = "([^"\n]+)"/.exec(engine)?.[1];
  assert.ok(liveDataMessage);
  assert.equal(match(liveDataMessage), 'network');
});

test('the real Cross Trade refusal predicate keeps old and new holds, and rejects unrelated errors', () => {
  const source = readFileSync(new URL('../../../../packages/safe-cex/src/crossTrade/runtime/CrossTradeManager.ts', import.meta.url), 'utf8');
  const declaration = /const isUncertainRefusal = [\s\S]*?;\n/.exec(source)?.[0];
  assert.ok(declaration);
  const isUncertainRefusal = new Function(`${stripTypeScriptTypes(declaration)}; return isUncertainRefusal;`)();
  assert.equal(isUncertainRefusal(new Error('Placement outcome is uncertain; reconcile before placing or amending')), true);
  assert.equal(isUncertainRefusal(new Error('An earlier order hasn’t been confirmed. Check your open orders before placing or editing orders.')), true);
  assert.equal(isUncertainRefusal(new Error('The exchange declined this order')), false);
  assert.equal(isUncertainRefusal('An earlier order hasn’t been confirmed'), false);
});

test('locale summary fragments preserve count and include one action for every presenter', () => {
  const { trade } = JSON.parse(readFileSync(new URL('../../../../apps/web/src/locales/en/trade.json', import.meta.url), 'utf8'));
  assert.equal(trade['placement-outcome-unknown_one'], '{{count}} order not confirmed. Check your open orders before trying again.');
  assert.equal(trade['placement-outcome-unknown_other'], '{{count}} orders not confirmed. Check your open orders before trying again.');
  assert.equal(trade['connection-status-bybitdemo-chaser-warning'], 'Bybit Demo order updates are not ready. Wait for live updates before starting a chase.');
  assert.equal(trade['hosted-server-switch-failed'], 'Accounts could not reconnect after the cloud settings changed');
  assert.equal(trade['breakeven-all-partial'], 'Breakeven stops: {{placed}} placed, {{failed}} not completed, {{unconfirmed}} not confirmed. Check your open orders before trying again.');
  for (const key of ['placement-outcome-unknown_one', 'placement-outcome-unknown_other']) {
    assert.equal((trade[key].match(/{{count}}/g) ?? []).length, 1);
    assert.equal(trade[key].split(guidance).length - 1, 1);
  }
});

test('both signing-failure wordings still prove no send despite a nested network message', async () => {
  const source = readFileSync(new URL('../../../../packages/safe-cex/src/utils/unknownSendOutcome.ts', import.meta.url), 'utf8');
  const url = `data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(source)).toString('base64')}`;
  const { isUnsentSigningFailure, isUnknownOutcome } = await import(url);
  for (const prefix of ['Remote signer unavailable', 'Signing isn’t available for this account']) {
    const message = `${prefix}: Network Error`;
    assert.equal(isUnsentSigningFailure(message), true);
    assert.equal(isUnknownOutcome({ message }), false);
  }
  assert.equal(isUnknownOutcome({ message: 'Network Error' }), true);
  assert.equal(isUnknownOutcome({ message: 'Signing isn’t available for this account', outcome: 'unknown' }), true);
});

// Rule 5: cleanup keeps waiting only for the same NotSent fence under either server wording.
test('scripted cleanup recognizes old and new cancel-all fences without admitting other failures', () => {
  const source = readFileSync(new URL('../../../harness/live-smoke.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('const waitingForRead =');
  const end = source.indexOf(' { failed = true; break; }', start);
  assert.ok(start >= 0 && end > start);
  const fragment = source.slice(start, end);
  const declarationEnd = fragment.indexOf(';') + 1;
  const condition = fragment.slice(declarationEnd).trim().slice('if ('.length, -1);
  const waiting = new Function('error', 'end', `${fragment.slice(0, declarationEnd)} return !(${condition});`);
  const oldMessage = 'Placement outcome is uncertain; reconcile before cancel-all';
  const newMessage = 'An earlier order hasn’t been confirmed. Check your open orders before cancelling all orders.';
  for (const message of [oldMessage, newMessage]) {
    assert.equal(waiting({ message, outcome: 'notSent' }, Infinity), true);
    assert.equal(waiting({ message, outcome: 'unknown' }, Infinity), false);
    assert.equal(waiting({ message, outcome: 'rejected' }, Infinity), false);
    assert.equal(waiting({ message, outcome: 'notSent' }, 0), false);
  }
  assert.equal(waiting({ message: 'Other refusal', outcome: 'notSent' }, Infinity), false);
});
