import { vi } from 'vitest';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { Exchange } from '../../exchanges/base';
import type { StoreWorkerPayload } from '../../webworker/StoreWorkerPayload';

import { afterEach, expect, it } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { deserializeCommandError, serializeCommandError } from '../../commandOutcome';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { CallMethodHandler } from '../../webworker/handlers/CallMethodHandler';

const exchanges: RustEngineExchange[] = [];
const handlers: CallMethodHandler[] = [];
afterEach(() => {
  handlers.splice(0).forEach((handler) => handler.dispose());
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
});
const approval = { code: 'GRVT_BUILDER_UNAUTHORIZED', reason: 'cap-too-low', requiredRaw: 300, authorizedRaw: 100 };

for (const [name, clone] of [
  ['structured clone', (value: StoreWorkerPayload) => structuredClone(value)],
  ['hosted JSON', (value: StoreWorkerPayload) => JSON.parse(JSON.stringify(value)) as StoreWorkerPayload],
] as const) {
  it.each([
    approval,
    { code: 'GRVT_BUILDER_UNAUTHORIZED', reason: 'unauthorized', requiredRaw: 300 },
  ])(`preserves $reason through worker sender and matching ${name} receiver`, async (approval) => {
    const main = new RustEngineExchange('grvt', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    exchanges.push(main);
    // Deliberately plain Error: worker protocol must not depend on a JS prototype.
    const original = Object.assign(new Error('Re-authorize builder'), approval, {
      kind: 'invalidInput',
      outcome: 'notSent',
      errorReported: true,
      cause: { credentials: 'must not be copied' },
    });
    const worker = {
      name: 'grvt',
      placeOrders: async () => {
        throw original;
      },
    } as unknown as Exchange;
    const replies: StoreWorkerPayload[] = [];
    const handler = new CallMethodHandler(
      'account',
      'grvt',
      worker,
      {
        postMessage: (message: StoreWorkerPayload) => {
          const copied = clone(message);
          replies.push(copied);
          main.emitter.emit('callMethod:done', { ...copied.data, requestId: 'unrelated' });
          main.emitter.emit('callMethod:done', copied.data);
        },
      } as unknown as Worker,
      {} as never,
      {} as never,
    );
    handlers.push(handler);
    main.emitter.on(
      'callMethod',
      ({ requestId, method, args }: { requestId: string; method: never; args: unknown[] }) => {
        void handler.handleCallMethod(requestId, method, args);
      },
    );
    const error = await main.callMethodInWorker('placeOrders', []).catch((error: unknown) => error);
    expect(error).toBeInstanceOf(Error);
    expect(error).toMatchObject({ ...approval, kind: 'invalidInput', outcome: 'notSent', errorReported: true });
    expect(error).not.toHaveProperty('cause');
    if (approval.reason === 'unauthorized') expect(error).not.toHaveProperty('authorizedRaw');
    expect(replies).toHaveLength(1);
    expect(replies[0].data).toMatchObject({ requestId: expect.any(String), success: false, result: approval });
  });
}

it('retains valid host builder metadata without reconstructing the old venue class', () => {
  const error = new ExchangeError('Approval missing', {
    message: 'Approval missing',
    builderAuthorization: { code: 'GRVT_BUILDER_UNAUTHORIZED', reason: 'unauthorized', requiredRaw: 300 },
    kind: 'invalidInput',
    outcome: 'notSent',
  });
  expect(deserializeCommandError(serializeCommandError(error))).toMatchObject({
    code: 'GRVT_BUILDER_UNAUTHORIZED',
    reason: 'unauthorized',
    requiredRaw: 300,
    kind: 'invalidInput',
    outcome: 'notSent',
  });
});

it.each([
  { ...approval, reason: 'pending' },
  { code: approval.code },
  { ...approval, authorizedRaw: 0.5 },
  { ...approval, requiredRaw: 0x1_0000_0000 },
  { ...approval, reason: 'unauthorized' },
  { ...approval, requiredRaw: -1 },
  { ...approval, authorizedRaw: 300 },
  { ...approval, authorizedRaw: NaN },
  { code: approval.code, reason: 'cap-too-low', requiredRaw: 300 },
])('does not turn malformed builder metadata into a modal-triggering error: %j', (record) => {
  const hostError = new ExchangeError('Approval could not be checked', { builderAuthorization: record } as never);
  expect(hostError).not.toHaveProperty('code');
  const copied = deserializeCommandError({ message: 'Could not check', ...record });
  expect(copied).not.toHaveProperty('code');
  expect(copied).not.toHaveProperty('reason');
  expect(copied).not.toHaveProperty('requiredRaw');
  expect(copied).not.toHaveProperty('authorizedRaw');
});
