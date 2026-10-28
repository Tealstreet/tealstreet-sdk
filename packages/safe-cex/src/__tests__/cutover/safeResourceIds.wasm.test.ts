// @vitest-environment node
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';

import { expect, it } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';

const actions = (text: string) => JSON.parse(text || '[]') as Array<{
  op: string; phase?: string; preparation: string; permit: string;
  packet: { request: number };
}>;

async function preparedRead() {
  const { WasmEngine } = await loadNodeEngine();
  const engine = new WasmEngine('binance', JSON.stringify({ publicDataOnly: true }));
  const preparation = actions(engine.start(0)).find((a) => a.op === 'managedHttp' && a.phase === 'prepare');
  expect(preparation).toBeDefined();
  expect(engine.httpPreparationPending!(preparation!.preparation)).toBe(true);
  return { engine, preparation: preparation! };
}

it.each([2 ** 32, Number.MAX_SAFE_INTEGER - 2])('wide HTTP ID offset %s never aliases a live low request', async (offset) => {
  const { engine, preparation } = await preparedRead();
  try {
    const bind = actions(engine.httpPrepared!(preparation.preparation, '1', undefined, 0))
      .find((a) => a.op === 'managedHttp' && a.phase === 'bind')!;
    expect(JSON.parse(engine.httpClaim!(bind.permit, 0))).toEqual({ status: 'released' });
    const before = JSON.parse(engine.health!()) as { httpFailures: number };
    expect(engine.httpFailed(preparation.packet.request + offset, 'unrelated wide identity', 1)).toBe('');
    expect((JSON.parse(engine.health!()) as { httpFailures: number }).httpFailures).toBe(before.httpFailures);
    engine.httpFailed(preparation.packet.request, 'the actual released request', 2);
    expect((JSON.parse(engine.health!()) as { httpFailures: number }).httpFailures).toBe(before.httpFailures + 1);
  } finally { engine.free!(); }
});

const ingress = [
  ['wsOpen', (id: number) => [id, 1e8]],
  ['wsMessage', (id: number) => [id, '{}', 1e8]],
  ['wsMessageActions', (id: number) => [id, '{}', 1e8]],
  // Resource validation precedes even the invalid UTF-8 early-return path.
  ['wsMessageBytes', (id: number) => [id, new Uint8Array([255]), 1e8]],
  ['wsBinary', (id: number) => [id, new Uint8Array(), 1e8]],
  ['wsClose', (id: number) => [id, 1006, 1e8]],
  ['httpStart', (id: number) => [id, 200, '', 1e8]],
  ['httpChunk', (id: number) => [id, new Uint8Array(), 1e8]],
  ['httpEnd', (id: number) => [id, 1e8]],
  ['httpFailed', (id: number) => [id, 'ignored', 1e8]],
  ['signed', (id: number) => [id, '{}', 1e8]],
  ['signFailed', (id: number) => [id, 'invalidResult', 1e8]],
  ['timer', (id: number) => [id, 1e8]],
  ['signingPending', (id: number) => [id]],
] as const;

for (const [name, argumentsFor] of ingress) {
  it.each([NaN, Infinity, -Infinity, -1, 1.5, 2 ** 53])(`${name} rejects malformed resource ID %s before retiring existing work`, async (id) => {
    const { engine, preparation } = await preparedRead();
    try {
      const entry = (engine as unknown as Record<string, (...args: unknown[]) => unknown>)[name];
      expect(typeof entry).toBe('function');
      let caught: unknown;
      try { entry.apply(engine as WasmEngineLike, argumentsFor(id)); } catch (error) { caught = error; }
      expect(caught).toBeInstanceOf(Error);
      expect(JSON.parse((caught as Error & { venueError: string }).venueError))
        .toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
      expect(engine.httpPreparationPending!(preparation.preparation)).toBe(true);
      // A rejected ingress with a far-future clock must not advance expiry.
      const bind = actions(engine.httpPrepared!(preparation.preparation, '1', undefined, 1))
        .find((a) => a.op === 'managedHttp' && a.phase === 'bind')!;
      expect(JSON.parse(engine.httpClaim!(bind.permit, 1))).toEqual({ status: 'released' });
    } finally { engine.free!(); }
  });
}
