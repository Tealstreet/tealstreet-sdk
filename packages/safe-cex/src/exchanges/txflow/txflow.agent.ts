/** Browser-owned TxFlow agent setup. This module never persists or recovers keys.
 * Schemas: https://docs.txflow-testnet.xyz/api/exchange/approveAgent.md
 * https://docs.txflow-testnet.xyz/api/info/approvedAgents.md
 * https://docs.txflow-testnet.xyz/api/signing.md
 */
import { ethers } from 'ethers';
import { virtualClock } from '../../utils/virtualClock';

import type { AgentWalletCreationResult } from '../hyperliquid/hyperliquid.agent';

const API = 'https://rpc.txflow-testnet.xyz';
export const TXFLOW_AGENT_DOMAIN = Object.freeze({
  name: 'TxFlow-Testnet', version: '1', chainId: 2511, verifyingContract: ethers.ZeroAddress,
});
export const TXFLOW_APPROVE_AGENT_TYPES = {
  ApproveAgent: [
    { name: 'txflowNetwork', type: 'string' }, { name: 'chainId', type: 'uint32' },
    { name: 'apiVersion', type: 'uint32' }, { name: 'agentAddress', type: 'address' },
    { name: 'agentName', type: 'string' }, { name: 'nonce', type: 'uint64' },
  ],
};

export interface TxflowAgentOptions {
  /** Must explicitly be true. Mainnet approval and trading are not supported. */
  testnet: boolean;
  agentName?: string;
  fetch?: typeof fetch;
}
export interface TxflowAgentApproval {
  type: 'approveAgent';
  txflowNetwork: 'TxFlow-Testnet';
  chainId: 2511;
  apiVersion: 1;
  agentAddress: string;
  agentName: string;
  nonce: number;
}
interface SignedAgentApproval {
  action: TxflowAgentApproval;
  signature: { r: string; s: string; v: number };
  nonce: number;
  signatureChainId?: number;
  confirm: 'executed';
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function array(value: unknown): value is unknown[] {
  return Array.isArray(value);
}

export type TxflowAgentAuthorization = { approved: false } | { approved: true; agentName: string };

function testnetOnly(testnet: boolean): void {
  if (testnet !== true) throw new Error('TxFlow requires testnet');
}

export function buildTxflowAgentApproval(
  agentAddress: string, agentName: string, nonce: number, testnet: boolean,
): TxflowAgentApproval {
  testnetOnly(testnet);
  if (!Number.isSafeInteger(nonce) || nonce < 0 || typeof agentName !== 'string' || [...agentName].length > 64) {
    throw new Error('Invalid TxFlow agent label or nonce');
  }
  return { type: 'approveAgent', txflowNetwork: 'TxFlow-Testnet', chainId: 2511, apiVersion: 1,
    agentAddress: ethers.getAddress(agentAddress), agentName, nonce };
}

export async function signTxflowAgentApproval(
  signer: ethers.Signer, action: TxflowAgentApproval, signatureChainId = 2511,
): Promise<SignedAgentApproval> {
  if (!Number.isInteger(signatureChainId) || signatureChainId < 1 || signatureChainId > 0xffffffff) {
    throw new Error('Invalid TxFlow signature chain');
  }
  const checked = buildTxflowAgentApproval(action.agentAddress, action.agentName, action.nonce,
    action.txflowNetwork === 'TxFlow-Testnet' && action.chainId === 2511 && action.apiVersion === 1);
  const { type: _type, ...message } = checked;
  const signed = ethers.Signature.from(await signer.signTypedData({ ...TXFLOW_AGENT_DOMAIN, chainId: signatureChainId }, TXFLOW_APPROVE_AGENT_TYPES, message));
  return { action: checked, signature: { r: signed.r, s: signed.s, v: signed.v },
    nonce: checked.nonce, ...(signatureChainId === 2511 ? {} : { signatureChainId }), confirm: 'executed' };
}

async function post(path: '/info' | '/exchange', body: unknown, options: TxflowAgentOptions): Promise<unknown> {
  testnetOnly(options.testnet);
  const response = await (options.fetch ?? fetch)(`${API}${path}`, {
    method: 'POST', headers: { 'Content-Type': 'application/json', 'x-api-version': '2026-08-16' },
    body: JSON.stringify(body),
  });
  // A 202 is still pending, never approval. Do not retry a possibly accepted write.
  if (response.status !== 200) throw new Error(`TxFlow ${path} failed or pending (${response.status})`);
  return response.json();
}

/** No local TTL is invented: absence includes revocation and venue-side expiry. */
export async function getTxflowAgentAuthorization(
  owner: string, agent: string, options: TxflowAgentOptions,
): Promise<TxflowAgentAuthorization> {
  testnetOnly(options.testnet);
  const user = ethers.getAddress(owner);
  const address = ethers.getAddress(agent);
  const response = await post('/info', { type: 'approvedAgents', user }, options);
  if (!record(response) || response.success !== true || response.code !== 0 || !record(response.data)
    || !array(response.data.agents) || typeof response.data.user !== 'string' || ethers.getAddress(response.data.user) !== user) {
    throw new Error('Invalid TxFlow agent authorization response');
  }
  let found: string | undefined;
  for (const row of response.data.agents) {
    if (!record(row) || typeof row.agentName !== 'string' || typeof row.agentAddress !== 'string') {
      throw new Error('Invalid TxFlow agent authorization row');
    }
    if (ethers.getAddress(row.agentAddress) === address) found = row.agentName;
  }
  return found === undefined ? { approved: false } : { approved: true, agentName: found };
}

async function approve(signer: ethers.Signer, action: TxflowAgentApproval, options: TxflowAgentOptions): Promise<void> {
  // Direct approval supports the documented domain-only override for wallets
  // that enforce their connected EVM chain. The action stays TxFlow testnet.
  const chainId = Number((await signer.provider?.getNetwork())?.chainId ?? 2511n);
  const response = await post('/exchange', await signTxflowAgentApproval(signer, action, chainId), options);
  if (!record(response) || response.status !== 'ok' || response.confirm !== 'executed' || response.code !== 0
    || !record(response.data) || response.data.type !== 'ApproveAgent' || !record(response.data.data)
    || !array(response.data.data.statuses) || response.data.data.statuses.length !== 1 || response.data.data.statuses[0] !== 'success') {
    throw new Error('TxFlow agent approval was not executed');
  }
}

async function ownerSigner(owner: string): Promise<ethers.JsonRpcSigner> {
  if (typeof window === 'undefined' || !window.ethereum) throw new Error('TxFlow requires a browser wallet');
  const provider = new ethers.BrowserProvider(window.ethereum);
  await provider.send('eth_requestAccounts', []);
  const signer = await provider.getSigner();
  if (ethers.getAddress(await signer.getAddress()) !== ethers.getAddress(owner)) {
    throw new Error('Connect the TxFlow account owner wallet');
  }
  return signer;
}

/** Returns the approved secret to the caller, exactly like HL. Storage remains caller-owned. */
export async function createTxflowAgentWithBrowserWallet(
  owner: string, options: TxflowAgentOptions,
): Promise<AgentWalletCreationResult> {
  testnetOnly(options.testnet);
  const signer = await ownerSigner(owner);
  const agent = ethers.Wallet.createRandom();
  const label = options.agentName ?? `Tealstreet ${agent.address.slice(2, 10)}`;
  await approve(signer, buildTxflowAgentApproval(agent.address, label, virtualClock.getCurrentTimeMs(), true), options);
  const authorized = await getTxflowAgentAuthorization(owner, agent.address, options);
  if (!authorized.approved) throw new Error('TxFlow agent is not approved');
  return { success: true, message: 'TxFlow agent approved', agentAddress: agent.address, agentPrivateKey: agent.privateKey };
}

export async function revokeTxflowAgentWithBrowserWallet(
  owner: string, agentAddress: string, agentName: string, options: TxflowAgentOptions,
): Promise<void> {
  testnetOnly(options.testnet);
  const signer = await ownerSigner(owner);
  const current = await getTxflowAgentAuthorization(owner, agentAddress, options);
  if (!current.approved || current.agentName !== agentName) throw new Error('TxFlow agent label does not match');
  await approve(signer, buildTxflowAgentApproval(ethers.ZeroAddress, agentName, virtualClock.getCurrentTimeMs(), true), options);
  if ((await getTxflowAgentAuthorization(owner, agentAddress, options)).approved) {
    throw new Error('TxFlow agent revocation is not confirmed');
  }
}
