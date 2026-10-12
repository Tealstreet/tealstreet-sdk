import { describe, expect, it } from 'vitest';

import { webhookBodyRefusal, webhookChainRefusal, webhookRefusal } from './webhookGuard';

describe('webhookRefusal', () => {
  it('lets trading verbs and script runs through', () => {
    for (const command of [
      'buy $100 BTCUSDT',
      'sell 1 ETHUSDT at 3000',
      'chase sell %all% 20% reduce',
      'close longs',
      'cancel',
      'script:rebalance',
      'sleep 1s',
    ]) {
      expect(webhookRefusal(command)).toBeNull();
    }
  });

  it('refuses a command by its alias as well as its name', () => {
    for (const command of [
      'simulation off',
      'sim off',
      'dry-run off',
      'account alt',
      'use alt',
      'acc alt',
      'symbol ETHUSDT',
      'sym ETHUSDT',
      'switch ETHUSDT',
      'accounts',
      'config',
    ]) {
      expect(webhookRefusal(command)).toMatch(/cannot be run from a webhook/);
    }
  });

  it('refuses persistent state, safety limits and standalone management verbs', () => {
    for (const command of [
      'set @size 5',
      'unset @size',
      'track max @x 1',
      'alias x "buy 1"',
      'when price > 1: buy',
      'fatfinger BTCUSDT 1',
      'budget 100',
      'whitelist BTCUSDT',
      'login',
      'whoami',
      'hook show abc',
      'webhook release',
      'export ~/.tealstreet/config.json',
      'import x.json',
      'script reload',
      'wa',
      'history',
    ]) {
      expect(webhookRefusal(command)).toMatch(/cannot be run from a webhook/);
    }
  });

  it('refuses a step that could be re-split into another command', () => {
    expect(webhookRefusal('buy 1 "x;when price > 1: sell"')).toMatch(/may not contain/);
    expect(webhookRefusal('@when price > 1: sell')).toMatch(/may not contain/);
  });

  it('refuses a body that could hold the listener for long', () => {
    expect(webhookBodyRefusal('repeat 1000 (repeat 1000 (buy 1))')).toMatch(/at most once/);
    expect(webhookBodyRefusal('repeat 3 (buy 1 BTCUSDT)')).toBeNull();
    expect(
      webhookChainRefusal([
        { command: 'sleep 30s', block: true },
        { command: 'sleep 31s', block: true },
      ]),
    ).toMatch(/60 seconds/);
    expect(webhookChainRefusal(Array.from({ length: 51 }, () => ({ command: 'buy 1' })))).toMatch(/50 steps/);
    expect(webhookChainRefusal([{ command: 'buy 1' }, { command: 'sleep 5s', block: true }])).toBeNull();
    expect(webhookChainRefusal([{ command: 'SLEEP 10m' }])).toMatch(/60 seconds/);
    expect(webhookRefusal('wait price > 100')).toMatch(/cannot be run from a webhook/);
  });
});
