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
  assert.equal(trade['placement-outcome-unknown_one'], '{{count}} order not confirmed. Check Open Orders and Positions before trying again.');
  assert.equal(trade['placement-outcome-unknown_other'], '{{count}} orders not confirmed. Check Open Orders and Positions before trying again.');
  assert.equal(trade['connection-status-bybitdemo-chaser-warning'], 'Bybit Demo order updates are not ready. Wait for live updates before starting a chase.');
  assert.equal(trade['hosted-server-switch-failed'], 'Accounts could not reconnect after the cloud settings changed');
  assert.equal(trade['breakeven-all-partial'], 'Breakeven stops: {{placed}} placed, {{failed}} not completed, {{unconfirmed}} not confirmed. Check your open orders before trying again.');
  for (const key of ['placement-outcome-unknown_one', 'placement-outcome-unknown_other']) {
    assert.equal((trade[key].match(/{{count}}/g) ?? []).length, 1);
    assert.equal(trade[key].split('before trying again.').length - 1, 1);
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

// Rule 5 (#3726): exercise the actual Rust display predicate/template; native unit tests cover the full functions in CI.
function rustDisplayDecorator(venue, functionName) {
  const source = readFileSync(new URL(`../../../venues/${venue}/src/trading.rs`, import.meta.url), 'utf8');
  const start = source.indexOf(`fn ${functionName ?? (venue === 'kraken' ? 'trading_failure' : 'terminal_failure_text')}(`);
  assert.ok(start >= 0);
  const tail = source.slice(start);
  const guard = /if (error\.outcome[\s\S]*?)\{/.exec(tail);
  assert.ok(guard);
  const condition = guard[1].replaceAll('Some(Outcome::Unknown)', "'unknown'").replaceAll('.contains(', '.includes(');
  const constant = (name) => new RegExp(`const ${name}: &str =\\s*"([^"\\n]+)"`).exec(source)?.[1];
  const decorate = new Function('error', 'UNKNOWN_TEXT', 'UNKNOWN_WRITE', `return (${condition});`);
  const template = /format!\(\s*"([^"\n]+)"/.exec(tail.slice(guard.index + guard[0].length))?.[1];
  assert.ok(template);
  return (error) => decorate(error, constant('UNKNOWN_TEXT'), constant('UNKNOWN_WRITE'))
    ? template.replace('{UNKNOWN_TEXT}', constant('UNKNOWN_TEXT')).replace('{UNKNOWN_WRITE}', constant('UNKNOWN_WRITE')).replace('{}', error.message)
    : error.message;
}

for (const venue of ['blofin', 'binance', 'kraken']) {
  test(`${venue} keeps already-guided Unknown display text unchanged across repeated decoration`, () => {
    const decorate = rustDisplayDecorator(venue);
    for (const message of [
      'We couldn’t confirm this change. Check your open orders before trying again.',
      'The exchange reply didn’t confirm this change. Check your open orders before trying again.',
      'The outcome of this order change is unknown. Check Open Orders',
      'Restoring the original wasn’t confirmed. Check your open orders before adding protection.',
    ]) {
      const error = { message, outcome: 'unknown' };
      assert.equal(decorate(error), message);
      assert.equal(decorate({ ...error, message: decorate(error) }), message);
      assert.equal((decorate(error).match(/Check (?:your open orders|Open Orders)/g) ?? []).length, 1);
    }
  });

  test(`${venue} decorates an unguided Unknown once and leaves ordinary refusals unchanged`, () => {
    const decorate = rustDisplayDecorator(venue);
    const error = { message: 'No reply arrived', outcome: 'unknown' };
    const message = decorate(error);
    assert.equal(message.split(guidance).length - 1, 1);
    for (const outcome of ['notSent', 'rejected', undefined]) {
      assert.equal(decorate({ message: 'Order declined', outcome }), 'Order declined');
    }
  });
}

function rustFunctionSource(path, name) {
  const source = readFileSync(new URL(path, import.meta.url), 'utf8');
  const start = source.indexOf(`fn ${name}(`);
  assert.ok(start >= 0, name);
  const end = source.indexOf('\n}', start);
  assert.ok(end > start, name);
  return source.slice(start, end + 2);
}

function bitgetCancelDisplay(message) {
  const path = '../../../venues/bitget/src/trading.rs';
  const body = rustFunctionSource(path, 'unproven_cancel_text');
  const source = readFileSync(new URL(path, import.meta.url), 'utf8');
  assert.ok(source.includes('VenueError::new(unproven_cancel_text(unproven))'));
  const guard = /if (message\.contains[\s\S]*?)\{/.exec(body)?.[1];
  if (guard && new Function('message', `return (${guard.replaceAll('.contains(', '.includes(')});`)(message)) return message;
  const template = /format!\(\s*"([^"\n]+)"/.exec(body)?.[1];
  assert.ok(template);
  return template.replace('{message}', message);
}

function bybitUnknownDisplay(error) {
  const body = rustFunctionSource('../../../venues/bybit/src/trading/ordinary.rs', 'unknown_outcome');
  const canonical = /let canonical =\s*"([^"\n]+)"/.exec(body)?.[1];
  const outer = /if (error\.outcome[^\n{]+)\{/.exec(body)?.[1];
  const inner = /if (!error\.message[\s\S]*?)\{/.exec(body)?.[1];
  const template = /format!\(\s*"([^"\n]+)"/.exec(body)?.[1];
  assert.ok(canonical && outer && inner && template);
  const condition = `(${outer}) && (${inner})`.replaceAll('Some(Outcome::Unknown)', "'unknown'").replaceAll('.contains(', '.includes(');
  return new Function('error', 'canonical', `return (${condition});`)(error, canonical)
    ? template.replace('{canonical}', canonical).replace('{}', error.message) : error.message;
}

function bitunixFinalDisplay(detail) {
  const source = readFileSync(new URL('../../../venues/bitunix/src/lib.rs', import.meta.url), 'utf8');
  const start = source.indexOf('fn command_failure_summary(');
  assert.ok(start >= 0);
  const summaryBody = source.slice(start, source.indexOf('\n    }', start));
  let summary = /"([^"\n]+)"/.exec(summaryBody)?.[1];
  assert.ok(summary);
  const body = rustFunctionSource('../../../engine/src/engine.rs', 'command_failure_message');
  const engine = readFileSync(new URL('../../../engine/src/engine.rs', import.meta.url), 'utf8');
  assert.ok(engine.includes('error.message = command_failure_message(&summary, &error.message)'));
  assert.ok(engine.includes('*report = command_failure_message(&summary, report)'));
  if (!detail) return summary;
  const guard = /if (detail\.contains[\s\S]*?)\{/.exec(body)?.[1];
  if (guard && new Function('detail', `return (${guard.replaceAll('.contains(', '.includes(')});`)(detail)) {
    const suffixes = [...body.matchAll(/summary\s*\.strip_suffix\(\s*"([^"\n]+)"\s*\)/g)].map((match) => match[1]);
    assert.ok(suffixes.length);
    for (const suffix of suffixes) if (summary.endsWith(suffix)) { summary = summary.slice(0, -suffix.length); break; }
  }
  const template = /format!\(\s*"([^"\n]+)"/.exec(body)?.[1];
  assert.ok(template);
  return template.replaceAll('\\n', '\n').replace('{summary}', summary).replace('{detail}', detail);
}

const actionCount = (text) => (text.match(/(?:Check|check) (?:your open orders|Open Orders)/g) ?? []).length;
// Rule 5 (#3726): final display composition retains native identity/context and one action.
test('Bitget keeps guided unproven cancellation text and its IDs', () => {
  const message = 'Bitget didn’t confirm cancellation of target (confirmed [kept]). Check your open orders before trying again.';
  assert.equal(bitgetCancelDisplay(message), message);
  assert.equal(actionCount(bitgetCancelDisplay(message)), 1);
});
test('Bitget adds one action when its cancellation detail is unguided', () => {
  assert.equal(actionCount(bitgetCancelDisplay('target (confirmed [kept])')), 1);
  assert.match(bitgetCancelDisplay('target (confirmed [kept])'), /target \(confirmed \[kept\]\)/);
});
test('Bybit retains the actual Hyro link and specific replacement guidance once', () => {
  const source = readFileSync(new URL('../../../venues/bybit/src/hyro.rs', import.meta.url), 'utf8');
  const template = /"(HyroTrader didn’t confirm order[^"\n]+)"/.exec(source)?.[1];
  assert.ok(template);
  const message = template.replace('{link}', 'retained-link').replace('{cause}', 'lost');
  assert.equal(bybitUnknownDisplay({ message, outcome: 'unknown' }), message);
  assert.equal(actionCount(bybitUnknownDisplay({ message, outcome: 'unknown' })), 1);
});
test('Bybit guides an unguided Unknown and leaves a refusal unchanged', () => {
  assert.equal(actionCount(bybitUnknownDisplay({ message: 'lost', outcome: 'unknown' })), 1);
  assert.equal(bybitUnknownDisplay({ message: 'declined', outcome: 'rejected' }), 'declined');
});
test('Bitunix keeps summary context but never repeats a guided terminal or warning action', () => {
  for (const detail of ['The exchange reply didn’t confirm this change. Check your open orders before trying again.', 'A previous change is unknown. Check Open Orders']) {
    const message = bitunixFinalDisplay(detail);
    assert.ok(message.endsWith(`\nContext: ${detail}`));
    assert.equal(actionCount(message), 1);
  }
});
test('Bitunix guides raw warning details and empty details without losing context', () => {
  assert.equal(actionCount(bitunixFinalDisplay('raw warning')), 1);
  assert.ok(bitunixFinalDisplay('raw warning').endsWith('\nContext: raw warning'));
  assert.equal(actionCount(bitunixFinalDisplay('')), 1);
});

function innerWriteDisplay(path, name, message) {
  const body = rustFunctionSource(path, name);
  const source = readFileSync(new URL(path, import.meta.url), 'utf8');
  const engine = readFileSync(new URL('../../../engine/src/unknown_outcome.rs', import.meta.url), 'utf8');
  const constant = (key) => new RegExp(`const ${key}: &str =\\s*"([^"\\n]+)"`).exec(`${source}\n${engine}`)?.[1];
  const guard = /if (message\.contains[\s\S]*?)\{/.exec(body)?.[1];
  if (guard && new Function('message', 'UNKNOWN_TEXT', 'UNKNOWN_OUTCOME_TEXT', `return (${guard.replaceAll('.contains(', '.includes(')});`)(message, constant('UNKNOWN_TEXT'), constant('UNKNOWN_OUTCOME_TEXT'))) return message;
  const template = /format!\(\s*"([^"\n]+)"/.exec(body)?.[1];
  assert.ok(template);
  return template.replace('{message}', message).replace('{UNKNOWN_TEXT}', constant('UNKNOWN_TEXT')).replace('{UNKNOWN_OUTCOME_TEXT}', constant('UNKNOWN_OUTCOME_TEXT'));
}

function asterWriteDisplay(error) {
  const body = rustFunctionSource('../../../venues/asterdex/src/trading.rs', 'write_failure_text');
  const source = readFileSync(new URL('../../../venues/asterdex/src/trading.rs', import.meta.url), 'utf8');
  const canonical = /const ENGINE_UNKNOWN_TEXT: &str =\s*"([^"\n]+)"/.exec(source)?.[1];
  const guided = /if (error\.message\.contains[\s\S]*?)\{/.exec(body)?.[1];
  const presented = /else if ([\s\S]*?)\{/.exec(body)?.[1];
  const template = /format!\(\s*"([^"\n]+)"/.exec(body)?.[1];
  assert.ok(canonical && guided && presented && template);
  if (error.outcome !== 'unknown') return error.message;
  const evaluate = (condition) => new Function('error', 'ENGINE_UNKNOWN_TEXT', `return (${condition.replaceAll('.contains(', '.includes(')});`)(error, canonical);
  if (evaluate(guided)) return error.message;
  return evaluate(presented) ? template.replace('{}', error.message).replace('{ENGINE_UNKNOWN_TEXT}', canonical) : canonical;
}

for (const [venue, path, name] of [
  ['Bitunix', '../../../venues/bitunix/src/trading.rs', 'unknown_failure_text'],
  ['Lighter', '../../../venues/lighter/src/trading.rs', 'protection_unknown_text'],
  ['WEEX', '../../../venues/weex/src/trading/context.rs', 'unknown_message'],
]) {
  test(`${venue} inner guidance keeps an existing engine HTTP action`, () => {
    const message = 'The exchange reply didn’t confirm this change. Check your open orders before trying again.';
    assert.equal(innerWriteDisplay(path, name, message), message);
    assert.equal(actionCount(innerWriteDisplay(path, name, message)), 1);
  });
  test(`${venue} inner guidance supplies one action for unguided details`, () => {
    const message = innerWriteDisplay(path, name, 'lost write');
    assert.match(message, /lost write/);
    assert.equal(actionCount(message), 1);
  });
}
test('BloFin secondary guidance preserves an already-guided engine error', () => {
  const decorate = rustDisplayDecorator('blofin', 'unknown_guidance');
  const message = 'The exchange reply didn’t confirm this change. Check your open orders before trying again.';
  assert.equal(decorate({ message, outcome: 'unknown', presents: true }), message);
});
test('BloFin secondary guidance still supplies an action to an unguided error', () => {
  const decorate = rustDisplayDecorator('blofin', 'unknown_guidance');
  assert.equal(actionCount(decorate({ message: 'lost', outcome: 'unknown', presents: true })), 1);
  assert.equal(decorate({ message: 'declined', outcome: 'rejected', presents: true }), 'declined');
});
test('Aster secondary guidance preserves retained legacy actions', () => {
  const message = 'The outcome is unknown. Check Open Orders';
  assert.equal(asterWriteDisplay({ message, outcome: 'unknown', presents: false }), message);
});
test('Aster secondary guidance still distinguishes unguided uncertainty and known refusal', () => {
  assert.equal(actionCount(asterWriteDisplay({ message: 'lost', outcome: 'unknown', presents: false })), 1);
  assert.equal(asterWriteDisplay({ message: 'declined', outcome: 'rejected', presents: false }), 'declined');
});
