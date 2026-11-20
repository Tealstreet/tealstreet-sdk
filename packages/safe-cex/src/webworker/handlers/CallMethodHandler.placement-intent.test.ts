import type { Exchange } from '../../exchanges/base';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { describe, expect, it, vi } from 'vitest';

import { delegatedIntentTime } from '../../delegatedOrderFrames';
import { capturePlacementIntent } from '../../placementIntentContext';
import { CallMethodHandler } from './CallMethodHandler';

const id = (n: number) => `019a9b43-1000-7000-8000-${n.toString(16).padStart(12, '0')}`;
const market = { symbol: 'BTCUSDT', type: 'market', side: 'buy', amount: 1000 };
const fixture = (send = vi.fn(async () => ['venue-order'])) => {
  const messages: unknown[] = [];
  const exchange = { name: 'bybit', placeOrder: send, fetchOrders: vi.fn(async () => []) };
  const handler = new CallMethodHandler('main', 'bybit', exchange as unknown as Exchange,
    { postMessage: (message: unknown) => { messages.push(message); } }, {} as never, {} as never);
  const deliver = (requestId: string, intentId: string, order = market) => handler.handle({
    op: 'callMethod', data: { requestId, intentId, method: 'placeOrder', args: [order] },
  });
  const replies = async (count: number) => {
    await vi.waitFor(() => expect(messages).toHaveLength(count));
    return messages;
  };
  return { handler, send, deliver, replies, messages, exchange };
};

describe('worker placement intent ingress', () => {
  it('three deliberate identical market actions produce three orders', async () => {
    const f = fixture();
    for (let n = 1; n <= 3; n++) f.deliver(`request-${n}`, id(n));
    await f.replies(3);
    expect(f.send).toHaveBeenCalledTimes(3);
    f.handler.dispose();
  });

  it('double delivery with two RPC request IDs joins one pending placement and replies to both', async () => {
    let finish = (_ids: string[]) => {};
    const pending = new Promise<string[]>((resolve) => { finish = resolve; });
    const send = vi.fn(() => pending);
    const f = fixture(send);
    f.deliver('first', id(1));
    f.deliver('duplicate', id(1));
    finish(['venue-order']);
    // Both listeners need a receipt; silently dropping the duplicate would hang it.
    await f.replies(2);
    expect(send).toHaveBeenCalledTimes(1);
    expect(f.messages).toEqual(['first', 'duplicate'].map((requestId) => ({
      op: 'callMethod:done', data: { requestId, success: true, result: ['venue-order'] },
    })));
    f.handler.dispose();
  });

  it('replaying a lost/Unknown placement never sends it again', async () => {
    const f = fixture(vi.fn<() => Promise<string[]>>().mockRejectedValueOnce(
      Object.assign(new Error('response lost'), { outcome: 'unknown' }),
    ).mockResolvedValue(['DUPLICATE']));
    f.deliver('first', id(1));
    await f.replies(1);
    f.deliver('reconnected', id(1));
    await f.replies(2);
    expect(f.send).toHaveBeenCalledTimes(1);
    expect(f.messages[1]).toMatchObject({ data: { success: false, result: { outcome: 'unknown' } } });
    f.handler.dispose();
  });

  it('refuses a changed order under a retained intent', async () => {
    const f = fixture();
    f.deliver('first', id(1));
    await f.replies(1);
    f.deliver('conflict', id(1), { ...market, amount: 2000 });
    await f.replies(2);
    expect(f.send).toHaveBeenCalledTimes(1);
    expect(f.messages[1]).toMatchObject({ data: { success: false, result: { outcome: 'unknown' } } });
    f.handler.dispose();
  });

  it('rejects invalid supplied identity and carries a valid identity to the writer', async () => {
    const seen: unknown[] = [];
    const f = fixture(vi.fn(async () => { seen.push(capturePlacementIntent()); return ['venue-order']; }));
    f.deliver('invalid', 'not-a-uuid');
    await f.replies(1);
    expect(f.send).not.toHaveBeenCalled();
    f.deliver('valid', id(1));
    await f.replies(2);
    expect(seen).toEqual([{ id: id(1), createdAt: delegatedIntentTime(id(1)) }]);
    f.handler.dispose();
  });

  it('a malformed redelivery cannot report notSent for an already admitted request', async () => {
    let finish = (_ids: string[]) => {};
    const pending = new Promise<string[]>((resolve) => { finish = resolve; });
    const f = fixture(vi.fn(() => pending));
    f.deliver('same-request', id(1));
    f.deliver('same-request', 'not-a-uuid');
    try {
      await f.replies(1);
      expect(f.messages[0]).toMatchObject({ data: { success: false, result: { outcome: 'unknown' } } });
      expect(f.send).toHaveBeenCalledTimes(1);
    } finally {
      finish(['venue-order']);
      await f.replies(2);
      f.handler.dispose();
    }
  });

  it('ordinary reads remain independent even when a caller repeats metadata', async () => {
    const f = fixture();
    const read: StoreWorkerPayload = { op: 'callMethod', data: { requestId: 'read', intentId: id(1), method: 'fetchOrders', args: [] } };
    f.handler.handle(read);
    f.handler.handle(read);
    await f.replies(2);
    expect(f.exchange.fetchOrders).toHaveBeenCalledTimes(2);
    f.handler.dispose();
  });
});
