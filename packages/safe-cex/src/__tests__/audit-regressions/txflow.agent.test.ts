import { ethers } from 'ethers';
import { virtualClock } from '../../utils/virtualClock';
import { directTransport } from '../../transport/direct';
import { setDefaultTransport } from '../../transport/registry';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  buildTxflowAgentApproval,
  createTxflowAgentWithBrowserWallet,
  getTxflowAgentAuthorization,
  revokeTxflowAgentWithBrowserWallet,
  signTxflowAgentApproval,
  TXFLOW_AGENT_DOMAIN,
  TXFLOW_APPROVE_AGENT_TYPES,
  type TxflowAgentApproval,
} from '../../exchanges/txflow/txflow.agent';

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); setDefaultTransport(directTransport); });
const ADDRESS = '0x0202020202020202020202020202020202020202';
const OWNER = '0x1111111111111111111111111111111111111111';
const action = () => buildTxflowAgentApproval(ADDRESS, 'my-trading-bot', 2000, true);

describe('TxFlow browser agent approval', () => {
  it('pins the published owner approval, independently of the trading digest', async () => {
    const { type: _type, ...message } = action();
    expect(ethers.TypedDataEncoder.hashStruct('ApproveAgent', TXFLOW_APPROVE_AGENT_TYPES, message))
      .toBe('0xbb5293971660f3252f455b6debbda781841fb23bd45cbbf0199c4de5250573b9');
    expect(ethers.TypedDataEncoder.hash(TXFLOW_AGENT_DOMAIN, TXFLOW_APPROVE_AGENT_TYPES, message))
      .toBe('0x1df22801b48ae1402ad36b244aa245fd6e842fa5fef2c40819b1e6c661cd2212');
    const signed = await signTxflowAgentApproval(new ethers.Wallet(`0x${'ab'.repeat(32)}`), action());
    expect(signed.signature).toEqual({
      r: '0x4431d4afb9fcd0dcd619afd55a7044cc26cb0ee1d436a1b06d836da217842ead',
      s: '0x54a0755ee3d8c241235a1828a2be732d72e9c152b5de25fc7c169966de4210f7', v: 27,
    });
    expect(signed).toMatchObject({ action: action(), nonce: 2000, confirm: 'executed' });
    expect(ethers.toBigInt(signed.signature.s)).toBeLessThanOrEqual(ethers.toBigInt('0x7fffffffffffffffffffffffffffffff5d576e7357a4501ddfe92f46681b20a0'));
  });

  it('refuses mainnet before wallet access or key generation', async () => {
    const random = vi.spyOn(ethers.Wallet, 'createRandom');
    await expect(createTxflowAgentWithBrowserWallet(OWNER, { testnet: false })).rejects.toThrow(/testnet/i);
    expect(random).not.toHaveBeenCalled();
    random.mockRestore();
  });

  it('never generates a server-side key', async () => {
    vi.stubGlobal('window', undefined);
    const random = vi.spyOn(ethers.Wallet, 'createRandom');
    await expect(createTxflowAgentWithBrowserWallet(OWNER, { testnet: true })).rejects.toThrow(/browser wallet/i);
    expect(random).not.toHaveBeenCalled();
    random.mockRestore();
  });

  it('uses the same label and the zero address for owner revocation', () => {
    expect(buildTxflowAgentApproval(ethers.ZeroAddress, 'my-trading-bot', 2001, true))
      .toMatchObject({ agentAddress: ethers.ZeroAddress, agentName: 'my-trading-bot', nonce: 2001 });
    expect(() => buildTxflowAgentApproval(ADDRESS, 'x'.repeat(65), 2000, true)).toThrow();
    expect(() => buildTxflowAgentApproval(ADDRESS, 'name', Number.NaN, true)).toThrow();
  });

  it('treats absent, revoked or expired approval as unauthorized, never as a new key', async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({
      success: true, code: 0, data: { user: OWNER, agents: [] },
    })));
    expect(await getTxflowAgentAuthorization(OWNER, ADDRESS, { testnet: true, fetch: fetcher })).toEqual({ approved: false });
    expect(fetcher.mock.calls[0][0]).toBe('https://rpc.txflow-testnet.xyz/info');
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ type: 'approvedAgents', user: OWNER });
  });

  it('only accepts approval of this agent by this account and fails closed on malformed reads', async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({
      success: true, code: 0, data: { user: OWNER, agents: [{ agentAddress: ADDRESS, agentName: 'browser' }] },
    })));
    expect(await getTxflowAgentAuthorization(OWNER, ADDRESS, { testnet: true, fetch: fetcher }))
      .toEqual({ approved: true, agentName: 'browser' });
    fetcher.mockResolvedValue(new Response(JSON.stringify({ success: true, code: 0, data: { agents: [] } })));
    await expect(getTxflowAgentAuthorization(OWNER, ADDRESS, { testnet: true, fetch: fetcher })).rejects.toThrow();
  });
});

describe('TxFlow client custody boundary', () => {
  it('returns a browser-created approved key to its caller without persistence or key disclosure', async () => {
    // Published signing-vector key; never a real account credential.
    const owner = new ethers.Wallet(`0x${'ab'.repeat(32)}`);
    let submitted: { action: TxflowAgentApproval } | undefined;
    const request = vi.fn(async ({ method, params }: { method: string; params?: unknown[] }) => {
      if (method === 'eth_chainId') return '0x9cf';
      if (method === 'eth_accounts' || method === 'eth_requestAccounts') return [owner.address];
      if (method === 'eth_signTypedData_v4') {
        const typed = JSON.parse(String(params?.[1]));
        delete typed.types.EIP712Domain;
        return owner.signTypedData(typed.domain, typed.types, typed.message);
      }
      throw new Error(`Unexpected wallet method ${method}`);
    });
    const setItem = vi.spyOn(Storage.prototype, 'setItem');
    vi.stubGlobal('window', { ethereum: { request } });
    const fetcher = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      const body: unknown = JSON.parse(String(init?.body));
      if (typeof body !== 'object' || body === null) throw new Error('Invalid request body');
      if ('action' in body) {
        const action = body.action;
        if (typeof action !== 'object' || action === null || !('agentAddress' in action)
          || typeof action.agentAddress !== 'string' || !('agentName' in action)
          || typeof action.agentName !== 'string' || !('nonce' in action) || typeof action.nonce !== 'number') {
          throw new Error('Invalid approval action');
        }
        submitted = { action: buildTxflowAgentApproval(action.agentAddress, action.agentName, action.nonce, true) };
        return new Response(JSON.stringify({ status: 'ok', code: 0, confirm: 'executed',
          data: { type: 'ApproveAgent', data: { statuses: ['success'] } } }));
      }
      const approval = submitted?.action;
      if (!approval) throw new Error('Approval must precede authorization lookup');
      return new Response(JSON.stringify({ success: true, code: 0,
        data: { user: owner.address, agents: [approval] } }));
    });
    const result = await createTxflowAgentWithBrowserWallet(owner.address, { testnet: true, fetch: fetcher });
    expect(result.success).toBe(true);
    if (!result.agentPrivateKey) throw new Error('Missing approved agent key');
    expect(new ethers.Wallet(result.agentPrivateKey).address).toBe(result.agentAddress);
    expect(JSON.stringify(fetcher.mock.calls)).not.toContain(result.agentPrivateKey);
    expect(JSON.stringify(submitted)).not.toContain('privateKey');
    expect(setItem).not.toHaveBeenCalled();
    expect(fetcher.mock.calls.every(([, init]) => new Headers(init?.headers).get('x-api-version') === '2026-08-16')).toBe(true);
    setItem.mockRestore();
  });
});

it('supports the documented wallet-domain override without changing the testnet action', async () => {
  const wallet = new ethers.Wallet(`0x${'ab'.repeat(32)}`);
  const payload = await signTxflowAgentApproval(wallet, action(), 1);
  expect(payload.signatureChainId).toBe(1);
  expect(payload.action).toMatchObject({ txflowNetwork: 'TxFlow-Testnet', chainId: 2511 });
  const { type: _type, ...message } = payload.action;
  expect(ethers.verifyTypedData({ ...TXFLOW_AGENT_DOMAIN, chainId: 1 }, TXFLOW_APPROVE_AGENT_TYPES,
    message, payload.signature)).toBe(wallet.address);
});


it.each([
  ['approve', 'override'], ['revoke', 'override'], ['approve', 'registered'], ['revoke', 'registered'],
] as const)('%s uses corrected nonce and the %s transport', async (operation, route) => {
  const owner = new ethers.Wallet(`0x${'ab'.repeat(32)}`);
  const raw = 1_800_000_000_000;
  const corrected = raw - 3_600_000;
  vi.spyOn(Date, 'now').mockReturnValue(raw);
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(corrected);
  const request = vi.fn(async ({ method, params }: { method: string; params?: unknown[] }) => {
    if (method === 'eth_chainId') return '0x9cf';
    if (method === 'eth_accounts' || method === 'eth_requestAccounts') return [owner.address];
    if (method === 'eth_signTypedData_v4') {
      const typed = JSON.parse(String(params?.[1]));
      delete typed.types.EIP712Domain;
      return owner.signTypedData(typed.domain, typed.types, typed.message);
    }
    throw new Error(`Unexpected wallet method ${method}`);
  });
  vi.stubGlobal('window', { ethereum: { request } });
  let submitted: TxflowAgentApproval | undefined;
  const fetcher = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
    const body: unknown = JSON.parse(String(init?.body));
    if (typeof body !== 'object' || body === null) throw new Error('Invalid request');
    if ('action' in body) {
      const action = body.action;
      if (typeof action !== 'object' || action === null || !('agentAddress' in action)
        || typeof action.agentAddress !== 'string' || !('agentName' in action) || typeof action.agentName !== 'string'
        || !('nonce' in action) || typeof action.nonce !== 'number') throw new Error('Invalid approval');
      submitted = buildTxflowAgentApproval(action.agentAddress, action.agentName, action.nonce, true);
      expect(body).toMatchObject({ nonce: corrected, action: { nonce: corrected } });
      return new Response(JSON.stringify({ status:'ok', code:0, confirm:'executed',
        data:{type:'ApproveAgent',data:{statuses:['success']}} }));
    }
    const agent = operation === 'approve' ? submitted : submitted ? undefined : {agentAddress:ADDRESS,agentName:'clock-test'};
    return new Response(JSON.stringify({success:true,code:0,data:{user:owner.address,agents:agent?[agent]:[]}}));
  });
  const resolveHeaders = vi.fn(async () => ({ Authorization: 'Bearer test-proxy' }));
  const privateProxy = vi.fn(() => ({ url: 'https://proxy.invalid', resolveHeaders }));
  setDefaultTransport({ ...directTransport, name: 'agent-test', privateProxy });
  vi.stubGlobal('fetch', fetcher);
  const options = { testnet: true, ...(route === 'override' ? { fetch: fetcher } : {}) };
  if (operation === 'approve') await createTxflowAgentWithBrowserWallet(owner.address, options);
  else await revokeTxflowAgentWithBrowserWallet(owner.address, ADDRESS, 'clock-test', options);
  if (route === 'registered') {
    expect(privateProxy).toHaveBeenCalledWith('txflow');
    expect(resolveHeaders).toHaveBeenCalledTimes(fetcher.mock.calls.length);
  } else expect(privateProxy).not.toHaveBeenCalled();
  for (const [url, init] of fetcher.mock.calls) {
    expect(String(url)).toMatch(route === 'registered'
      ? /^https:\/\/proxy\.invalid\/https:\/\/rpc\.txflow-testnet\.xyz\/(info|exchange)$/
      : /^https:\/\/rpc\.txflow-testnet\.xyz\/(info|exchange)$/);
    expect(init?.headers).toMatchObject({ 'Content-Type': 'application/json', 'x-api-version': '2026-08-16',
      ...(route === 'registered' ? { Authorization: 'Bearer test-proxy' } : {}) });
  }
  expect(submitted?.nonce).toBe(corrected);
});


it('refuses missing configured proxy authorization without falling back to the venue', async () => {
  const fetcher = vi.fn().mockRejectedValue(new Error('Unexpected direct request'));
  vi.stubGlobal('fetch', fetcher);
  setDefaultTransport({ ...directTransport, name: 'agent-test', privateProxy: () => ({
    url: 'https://proxy.invalid', resolveHeaders: async () => ({}),
  }) });
  await expect(getTxflowAgentAuthorization(OWNER, ADDRESS, { testnet: true }))
    .rejects.toThrow('TxFlow agent proxy authorization is unavailable');
  expect(fetcher).not.toHaveBeenCalled();
});
