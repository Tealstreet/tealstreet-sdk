import type { FetchedLeverage } from '../../generated/FetchedLeverage';

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { Script } from 'node:vm';

import ts from 'typescript';
import { expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { nativeFixture } from '../cutover/nativeFixture';

// Execute the production arrow methods without loading a WASM engine or a signer.
const path = resolve(__dirname, '../../exchanges/rust/rust.exchange.ts');
const source = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true);
const exchange = source.statements.find(
  (node): node is ts.ClassDeclaration => ts.isClassDeclaration(node) && node.name?.text === 'RustEngineExchange',
);
function method(name: string, receiver: object): (symbol: string) => Promise<unknown> {
  const node = exchange?.members.find(
    (member): member is ts.PropertyDeclaration =>
      ts.isPropertyDeclaration(member) && member.name.getText(source) === name,
  );
  if (!node?.initializer) throw new Error(`Missing production method ${name}`);
  const js = ts.transpileModule(`(function () { return (${node.initializer.getText(source)}); })`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const factory = new Script(js).runInNewContext({ LOADED_FLAG: { MARKETS: 1 }, LogSeverity: { Warning: 2 } });
  return factory.call(receiver);
}
function receiver(result: FetchedLeverage | null = null) {
  const store = new FrameStore();
  store.ingest(
    nativeFixture({
      markets: [
        {
          id: 'BTCUSDT',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 1, price: 1 },
          limits: { amount: { min: 0, max: 100 }, leverage: { min: 1, max: 100 } },
        },
      ],
    }),
  );
  const capabilities = { leverageQuery: 'onReadAndMetadata', maxLeverage: 'market' };
  return {
    isPublicDataOnly: false,
    startupState: 'started',
    isDisposed: false,
    routesToWorker: false,
    capabilityRecord: capabilities,
    capabilitiesFor: () => capabilities,
    captureWorkerAdmission: () => undefined,
    untilNativeData: vi.fn(async () => {}),
    store,
    command: vi.fn(async () => result),
    host: {
      flush: vi.fn(() => {
        if (result) store.ingest(nativeFixture({ fetchedLeverage: { BTCUSDT: result } }, ['BTCUSDT']));
      }),
    },
    log: vi.fn(),
  };
}
it.each(['updateSymbolMetadata', 'fetchLeverageForSymbol'])(
  '%s retains optional leverage absence without manufacturing rails or warning',
  async (name) => {
    const context = receiver();
    const result = await method(name, context)('BTCUSDT');
    expect(result).toBe(name === 'fetchLeverageForSymbol' ? null : undefined);
    expect(context.command).toHaveBeenCalledWith({ op: 'fetchLeverage', symbol: 'BTCUSDT' }, { report: false });
    expect(context.store.fetchedLeverage).toEqual({});
    expect(context.log).not.toHaveBeenCalled();
  },
);
it('keeps populated leverage and real read failures observable', async () => {
  const rails = { buyLeverage: 12, sellLeverage: 8 };
  const context = receiver(rails);
  await method('updateSymbolMetadata', context)('BTCUSDT');
  expect(context.store.fetchedLeverage.BTCUSDT).toMatchObject(rails);
  context.command.mockRejectedValueOnce(new Error('Bitget leverage read HTTP 503'));
  await method('updateSymbolMetadata', context)('BTCUSDT');
  expect(context.log).toHaveBeenCalledWith(expect.stringContaining('HTTP 503'), 2);
});
it('a scalar maximum caller gets an explicit failure rather than a null dereference', async () => {
  const context = receiver();
  context.capabilityRecord.maxLeverage = 'rails';
  await expect(method('fetchSymbolMaxLeverage', context)('BTCUSDT')).rejects.toThrow('Leverage is not available');
});
