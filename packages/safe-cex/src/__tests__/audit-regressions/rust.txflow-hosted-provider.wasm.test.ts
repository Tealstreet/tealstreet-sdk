import type { SignRequest, SignResult } from '@tealstreet/safe-cex-wasm/host';
import type { RemoteSigner } from '../../types';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import { ethers } from 'ethers';
import { afterEach, expect, it, vi } from 'vitest';

import { deriveSignerSetupPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { mapHyperliquidSignerSetup } from '../../exchanges/rust/hyperliquidSignerSetup';
import { LocalSignerSetup } from '../../exchanges/rust/localSignerSetup';
import { actionHash, agentDigest } from '../reference/txflow/txflow.signing';
import { prepareRecordedHttp } from '../support/managedHttp';

const OWNER = '0x0000000000000000000000000000000000000001';
const SECRET = `0x${'ab'.repeat(32)}`; // Public disposable signing vector, never funded.
const AGENT = new ethers.Wallet(SECRET).address;
const STAMP = 1_700_000_000_000;
afterEach(() => vi.useRealTimers());

async function origin(tamperReceipt = false) {
  vi.useFakeTimers(); vi.setSystemTime(STAMP);
  const { WasmEngine } = await loadNodeEngine();
  const options = { testnet: true, key: AGENT, applicationId: OWNER };
  const local = new LocalSignerSetup(mapHyperliquidSignerSetup({ ...options, secret: SECRET }, 'TxFlow'), deriveSignerSetupPolicy('txflow'));
  const device = local.construct(WasmEngine, 'txflow', JSON.stringify(options));
  const sign = vi.fn<RemoteSigner['sign']>(async (request) => {
    if (!request.wsAuth || !device.hostedWalletSigning) throw new Error('Missing owned wallet work');
    return { id: request.id, wsAuthSignature: device.hostedWalletSigning(
      request.wsAuth.toSign, request.wsAuth.outputFormat, Date.now(),
    ) };
  });
  const setup = new DelegatedSignerSetup('txflow', { ...options, secret: '', remoteSigner: {
    sign, isAvailable: () => true, onAvailabilityChange: () => () => {},
  } });
  const engine = setup.construct(WasmEngine, 'txflow', JSON.stringify(options));
  const work: SignRequest[] = [];
  const results: SignResult[] = [];
  const sent: Record<string, unknown>[] = [];
  let openOrders: unknown[] = [];
  const host = new EngineHost(engine, {
    now: () => Date.now(), prepareHttp: prepareRecordedHttp,
    request: async ({ url, body }) => {
      const request = JSON.parse(String(body));
      let data: unknown;
      if (url.endsWith('/exchange')) {
        sent.push(request);
        const { type: _type, ...action } = request.action;
        expect(ethers.recoverAddress(agentDigest(actionHash(action, null, request.nonce)), request.signature)).toBe(AGENT);
        openOrders = request.action.orders.map((order: { c: string }, index: number) => ({
          asset: 1, oid: 9001 + index, side: 'B', orderType: 'Limit', limitPx: '100', origSz: '0.01', sz: '0.01',
          timestamp: Date.now(), isTrigger: false, reduceOnly: false, cloid: order.c, state: 'Open', tif: 'GTC', children: [],
        }));
        return { status: 200, headers: '', body: new TextEncoder().encode(JSON.stringify({
          status: 'ok', code: 0, confirm: 'executed', data: { type: 'PlaceOrder', data: { statuses: ['success'] } },
        })) };
      }
      switch (request.type) {
        case 'perpMeta': data = { universe: [{ index: 1, name: 'BTC-USDC', baseCurrency: 'BTC', quoteCurrency: 'USDC',
          currencyType: 'usdc', delisted: false, haltTrading: false, basePrecision: '0.001', priceTick: '0.1', maxLeverage: 50 }] }; break;
        case 'activeAssetCtx': data = { instrumentId: 1, markPx: '100', lastPrice: '100' }; break;
        case 'clearinghouseState': data = { user: OWNER, time: Date.now(), assetPositions: [],
          marginSummary: { accountValue: '1000', totalMarginUsed: '0', totalRawUsd: '1000' } }; break;
        case 'frontendOpenOrders': data = openOrders; break;
        case 'userFills': data = { data: [], hasMore: false }; break;
        case 'activeAssetData': data = { user: OWNER, coin: '1', leverage: { type: 'cross', mode: 'oneWay', value: 10 } }; break;
        case 'approvedAgents': data = { user: OWNER, agents: [{ agentAddress: AGENT, agentName: 'fixture' }] }; break;
        default: throw new Error(`Unexpected native request ${request.type}`);
      }
      return { status: 200, headers: '', body: new TextEncoder().encode(JSON.stringify({ success: true, code: 0, data })) };
    },
    connect: (_url, handlers) => { queueMicrotask(() => handlers.onOpen()); return { send: () => {}, close: () => {} }; },
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }, {}, { ownsEngine: true, entropy: (n) => new Uint8Array(n).fill(7), signingEntropy: (n) => new Uint8Array(n).fill(9),
    signerAlgorithms: setup.signerAlgorithms, signer: async (request, context) => {
      work.push(structuredClone(request));
      const result = await setup.signer(request, context);
      results.push(structuredClone(result));
      return tamperReceipt && request.receipt.nonce !== undefined
        ? { ...result, receipt: { ...result.receipt, nonce: String(BigInt(request.receipt.nonce) + 1n) } } : result;
    } });
  await host.prepareAddressSetup(); host.start();
  await vi.advanceTimersByTimeAsync(500);
  return { host, setup, device, sign, work, results, sent, dispose: () => { host.dispose(); setup.dispose(); device.free?.(); } };
}

it('native TxFlow mutation reaches the production provider and owned device with its exact nonce receipt', async () => {
  const fixture = await origin();
  try {
    const done = fixture.host.command({ op: 'placeOrders', orders: [{ symbol: 'BTC-USDC', type: 'limit', side: 'buy',
      amount: 0.01, price: 100, clientOrderId: 'hosted-one' }] }).then(value => ({ value }), error => ({ error }));
    await vi.advanceTimersByTimeAsync(3_000);
    expect(fixture.sign.mock.calls.filter(([request]) => request.wsAuth?.outputFormat === 'txflow-agent')).toHaveLength(1);
    expect(fixture.sent).toHaveLength(1);
    expect(await done).toEqual({ value: ['9001'] });
    const index = fixture.work.findIndex(request => request.receipt.nonce !== undefined);
    expect(fixture.work[index].receipt).toMatchObject({ stampMs: expect.any(Number), nonce: expect.stringMatching(/^[1-9][0-9]*$/) });
    expect(fixture.results[index].receipt).toEqual(fixture.work[index].receipt);
    const calls = fixture.sign.mock.calls.length;
    for (const nonce of ['01', '-1', '1.0', '18446744073709551616', 1]) {
      await expect(fixture.setup.signer({ ...fixture.work[index], receipt: { ...fixture.work[index].receipt, nonce } } as SignRequest,
        { signal: new AbortController().signal })).rejects.toThrow();
    }
    expect(fixture.sign).toHaveBeenCalledTimes(calls);
  } finally { fixture.dispose(); }
});

it('native apply refuses a device result whose echoed nonce receipt was changed', async () => {
  const fixture = await origin(true);
  try {
    const done = fixture.host.command({ op: 'placeOrders', orders: [{ symbol: 'BTC-USDC', type: 'limit', side: 'buy',
      amount: 0.01, price: 100, clientOrderId: 'changed-receipt' }] }).catch(error => error);
    await vi.advanceTimersByTimeAsync(3_000);
    expect(fixture.sign.mock.calls.filter(([request]) => request.wsAuth?.outputFormat === 'txflow-agent')).toHaveLength(1);
    expect(fixture.sent).toHaveLength(0);
    expect(await done).toBeInstanceOf(Error);
  } finally { fixture.dispose(); }
});
