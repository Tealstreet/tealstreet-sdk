import { ethers } from 'ethers';

import { actionHash } from '../hyperliquid/hyperliquid.signing';

export { actionHash } from '../hyperliquid/hyperliquid.signing';

export const TXFLOW_DOMAIN = {
  name: 'TxFlow-Testnet',
  version: '1',
  chainId: 2511,
  verifyingContract: ethers.ZeroAddress,
};

const agentTypes = {
  Agent: [
    { name: 'txflowNetwork', type: 'string' },
    { name: 'chainId', type: 'uint32' },
    { name: 'apiVersion', type: 'uint32' },
    { name: 'connectionId', type: 'bytes32' },
  ],
};

// The caller passes the typed action fields without the outer dispatch tag.
export function agentMessage(connectionId: string) {
  return { txflowNetwork: TXFLOW_DOMAIN.name, chainId: TXFLOW_DOMAIN.chainId, apiVersion: 1, connectionId };
}

export function agentDigest(connectionId: string): string {
  return ethers.TypedDataEncoder.hash(TXFLOW_DOMAIN, agentTypes, agentMessage(connectionId));
}

export async function signAction(action: unknown, nonce: number, secret: string, vault?: string | null) {
  if (!Number.isSafeInteger(nonce) || nonce < 0) throw new Error('Invalid TxFlow nonce');
  if (vault != null) ethers.getAddress(vault);
  const connectionId = actionHash(action, vault ?? null, nonce);
  const signature = ethers.Signature.from(await new ethers.Wallet(secret).signTypedData(
    TXFLOW_DOMAIN, agentTypes, agentMessage(connectionId),
  ));
  return { r: signature.r, s: signature.s, v: signature.v };
}
