import assert from 'node:assert/strict';
import test from 'node:test';

import tsParser from '@typescript-eslint/parser';
import { Linter } from 'eslint';

import safeCexPlugin from './no-discarded-fetch-result.mjs';

const linter = new Linter({ configType: 'flat' });
const config = {
  files: ['**/*.ts'],
  languageOptions: {
    parser: tsParser,
    parserOptions: { ecmaVersion: 'latest', sourceType: 'module' },
  },
  plugins: { 'safe-cex': safeCexPlugin },
  rules: { 'safe-cex/no-discarded-fetch-result': 'error' },
};

const lint = (code) => linter.verify(code, config, { filename: 'exchange.ts' });

test('rejects bare, awaited, and voided reconciliation fetches', () => {
  const messages = lint(`
    async function reconcile() {
      this.fetchOrders();
      await this.parent.fetchPositions(['BTC-USDT']);
      void this.fetchBalance();
    }
  `);

  assert.deepEqual(
    messages.map(({ messageId, line }) => [messageId, line]),
    [
      ['discarded', 3],
      ['discarded', 4],
      ['discarded', 5],
    ],
  );
});

test('rejects catch/finally chains and then callbacks that ignore the fetched value', () => {
  const messages = lint(`
    async function reconcile() {
      this.fetchOrders().catch(reportError);
      this.fetchPositions().finally(markDone);
      this.fetchBalance().then(() => refreshSomethingElse());
    }
  `);

  assert.deepEqual(messages.map(({ line }) => line), [3, 4, 5]);
});

test('rejects discarded Promise.all reconciliation results', () => {
  const messages = lint(`
    async function reconcile() {
      await Promise.all([this.fetchOrders(), this.fetchPositions()]);
      void Promise.allSettled([this.fetchBalance()]);
    }
  `);

  assert.deepEqual(messages.map(({ line }) => line), [3, 3, 4]);
});

test('tracks Promise.all consumption per result', () => {
  const messages = lint(`
    async function reconcile() {
      Promise.all([this.fetchOrders(), this.fetchPositions()]).then(([orders]) => commitOrders(orders));
      Promise.all([this.fetchBalance(), this.fetchPositions()]).then(([balance, positions]) => {
        commitBalance(balance);
        commitPositions(positions);
      });
    }
  `);

  assert.deepEqual(messages.map(({ line }) => line), [3]);
});

test('allows assigned, returned, stored, and callback-consumed results', () => {
  const messages = lint(`
    async function reconcile() {
      const orders = await this.fetchOrders();
      this.store.update({ positions: await this.fetchPositions() });
      consume(await this.fetchBalance());
      this.fetchOrders().then((nextOrders) => this.store.update({ orders: nextOrders }));
      await Promise.all([
        this.fetchOrders().then((nextOrders) => this.store.update({ orders: nextOrders })),
      ]);
      await Promise.all([consume(this.fetchOrders())]);
      this.fetchBalance().then(({ free }) => this.store.update({ free }));
      const [nextPositions, nextBalance] = await Promise.all([
        this.fetchPositions(),
        this.fetchBalance(),
      ]);
      return { orders, nextPositions, nextBalance };
    }
  `);

  assert.equal(messages.length, 0);
});
