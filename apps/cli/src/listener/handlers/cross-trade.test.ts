import type { StoredCrossTradeConfig } from '../../cross-trade-store';

import { describe, it, expect, vi, beforeEach } from 'vitest';

const sendMock = vi.fn();
const fakeWs = { send: sendMock } as unknown as Parameters<typeof handleMutateCrossTrade>[0];

const loadCrossTradeFileMock = vi.fn();
const upsertCrossTradeConfigRawMock = vi.fn();
const tombstoneCrossTradeConfigMock = vi.fn();

vi.mock('../../cross-trade-store', () => ({
  loadCrossTradeFile: () => loadCrossTradeFileMock(),
  upsertCrossTradeConfigRaw: (...args: unknown[]) => upsertCrossTradeConfigRawMock(...args),
  tombstoneCrossTradeConfig: (...args: unknown[]) => tombstoneCrossTradeConfigMock(...args),
}));

vi.mock('../../logger', () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}));

import { handleMutateCrossTrade } from './cross-trade';

const makeRow = (overrides: Partial<StoredCrossTradeConfig> = {}): StoredCrossTradeConfig => ({
  id: 'row-1',
  enabled: true,
  masterAccount: 'master',
  masterSymbol: 'BTCUSDT',
  masterWeight: 1,
  copiers: [
    {
      enabled: true,
      account: 'copier',
      weight: 1,
      execution: 'Market',
      weightMode: 'Absolute',
      symbolMappings: {},
    },
  ],
  updatedAt: 1000,
  ...overrides,
});

describe('handleMutateCrossTrade', () => {
  beforeEach(() => {
    sendMock.mockReset();
    loadCrossTradeFileMock.mockReset();
    upsertCrossTradeConfigRawMock.mockReset();
    tombstoneCrossTradeConfigMock.mockReset();
    loadCrossTradeFileMock.mockReturnValue({ configs: [] });
  });

  it('rejects frames missing id', () => {
    const result = handleMutateCrossTrade(fakeWs, { t: 'mutate:cross-trade-config' });
    expect(result).toEqual({ acked: false, wrote: false });
    expect(sendMock).not.toHaveBeenCalled();
  });

  describe('upsert', () => {
    it('writes when no existing row and acks', () => {
      const row = makeRow({ updatedAt: 5000 });
      const result = handleMutateCrossTrade(fakeWs, {
        t: 'mutate:cross-trade-config',
        id: 'm-1',
        op: 'upsert',
        config: row,
      });
      expect(result).toEqual({ acked: true, wrote: true });
      expect(upsertCrossTradeConfigRawMock).toHaveBeenCalledWith(row);
      expect(sendMock).toHaveBeenCalledWith(JSON.stringify({ t: 'mutate:ack', id: 'm-1' }));
    });

    it('writes when incoming.updatedAt > existing.updatedAt', () => {
      loadCrossTradeFileMock.mockReturnValue({ configs: [makeRow({ updatedAt: 1000 })] });
      const incoming = makeRow({ updatedAt: 2000, enabled: false });
      const result = handleMutateCrossTrade(fakeWs, {
        t: 'mutate:cross-trade-config',
        id: 'm-1',
        op: 'upsert',
        config: incoming,
      });
      expect(result).toEqual({ acked: true, wrote: true });
      expect(upsertCrossTradeConfigRawMock).toHaveBeenCalledWith(incoming);
    });

    it('acks without write when incoming is stale (existing.updatedAt >= incoming)', () => {
      loadCrossTradeFileMock.mockReturnValue({ configs: [makeRow({ updatedAt: 2000 })] });
      const incoming = makeRow({ updatedAt: 1000 });
      const result = handleMutateCrossTrade(fakeWs, {
        t: 'mutate:cross-trade-config',
        id: 'm-1',
        op: 'upsert',
        config: incoming,
      });
      expect(result).toEqual({ acked: true, wrote: false });
      expect(upsertCrossTradeConfigRawMock).not.toHaveBeenCalled();
      expect(sendMock).toHaveBeenCalledWith(JSON.stringify({ t: 'mutate:ack', id: 'm-1' }));
    });

    it('rejects upsert with updatedAt <= 0 (LWW: blocks stale-write overwrite)', () => {
      loadCrossTradeFileMock.mockReturnValue({ configs: [makeRow({ updatedAt: 5000 })] });
      const stale = makeRow({ updatedAt: 0 });
      const result = handleMutateCrossTrade(fakeWs, {
        t: 'mutate:cross-trade-config',
        id: 'm-1',
        op: 'upsert',
        config: stale,
      });
      expect(result).toEqual({ acked: true, wrote: false });
      expect(upsertCrossTradeConfigRawMock).not.toHaveBeenCalled();
      expect(sendMock).toHaveBeenCalledWith(
        JSON.stringify({
          t: 'mutate:err',
          id: 'm-1',
          error: 'mutate:cross-trade-config upsert missing or invalid config.updatedAt',
        }),
      );
    });

    it('rejects upsert missing config.id', () => {
      const result = handleMutateCrossTrade(fakeWs, {
        t: 'mutate:cross-trade-config',
        id: 'm-1',
        op: 'upsert',
        config: { ...makeRow(), id: '' },
      });
      expect(result).toEqual({ acked: true, wrote: false });
      expect(upsertCrossTradeConfigRawMock).not.toHaveBeenCalled();
      expect(sendMock).toHaveBeenCalledWith(
        JSON.stringify({ t: 'mutate:err', id: 'm-1', error: 'mutate:cross-trade-config upsert missing config.id' }),
      );
    });
  });

  describe('delete', () => {
    it('tombstones when no existing row exists', () => {
      const result = handleMutateCrossTrade(fakeWs, {
        t: 'mutate:cross-trade-config',
        id: 'm-1',
        op: 'delete',
        configId: 'row-1',
        updatedAt: 5000,
      });
      expect(result).toEqual({ acked: true, wrote: true });
      expect(tombstoneCrossTradeConfigMock).toHaveBeenCalledWith('row-1', 5000);
    });

    it('tombstones when incoming.updatedAt > existing.updatedAt', () => {
      loadCrossTradeFileMock.mockReturnValue({ configs: [makeRow({ updatedAt: 1000 })] });
      const result = handleMutateCrossTrade(fakeWs, {
        t: 'mutate:cross-trade-config',
        id: 'm-1',
        op: 'delete',
        configId: 'row-1',
        updatedAt: 2000,
      });
      expect(result).toEqual({ acked: true, wrote: true });
      expect(tombstoneCrossTradeConfigMock).toHaveBeenCalledWith('row-1', 2000);
    });

    it('acks without write when delete is stale', () => {
      loadCrossTradeFileMock.mockReturnValue({ configs: [makeRow({ updatedAt: 5000 })] });
      const result = handleMutateCrossTrade(fakeWs, {
        t: 'mutate:cross-trade-config',
        id: 'm-1',
        op: 'delete',
        configId: 'row-1',
        updatedAt: 1000,
      });
      expect(result).toEqual({ acked: true, wrote: false });
      expect(tombstoneCrossTradeConfigMock).not.toHaveBeenCalled();
    });

    it('rejects delete with updatedAt <= 0', () => {
      const result = handleMutateCrossTrade(fakeWs, {
        t: 'mutate:cross-trade-config',
        id: 'm-1',
        op: 'delete',
        configId: 'row-1',
        updatedAt: 0,
      });
      expect(result).toEqual({ acked: true, wrote: false });
      expect(tombstoneCrossTradeConfigMock).not.toHaveBeenCalled();
      expect(sendMock).toHaveBeenCalledWith(
        JSON.stringify({
          t: 'mutate:err',
          id: 'm-1',
          error: 'mutate:cross-trade-config delete missing or invalid updatedAt',
        }),
      );
    });

    it('rejects delete missing configId', () => {
      const result = handleMutateCrossTrade(fakeWs, { t: 'mutate:cross-trade-config', id: 'm-1', op: 'delete' });
      expect(result).toEqual({ acked: true, wrote: false });
      expect(tombstoneCrossTradeConfigMock).not.toHaveBeenCalled();
      expect(sendMock).toHaveBeenCalledWith(
        JSON.stringify({ t: 'mutate:err', id: 'm-1', error: 'mutate:cross-trade-config delete missing configId' }),
      );
    });
  });

  it('rejects unknown op', () => {
    const result = handleMutateCrossTrade(fakeWs, { t: 'mutate:cross-trade-config', id: 'm-1', op: 'wat' });
    expect(result).toEqual({ acked: true, wrote: false });
    expect(sendMock).toHaveBeenCalledWith(
      JSON.stringify({ t: 'mutate:err', id: 'm-1', error: 'mutate:cross-trade-config unknown op: wat' }),
    );
  });
});
