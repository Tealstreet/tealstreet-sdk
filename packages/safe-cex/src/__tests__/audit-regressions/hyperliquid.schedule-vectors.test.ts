import { encode } from '@msgpack/msgpack';
import { ethers } from 'ethers';
import { expect, it } from 'vitest';
import vectors from '../fixtures/hyperliquidScheduleCancel.json';
import { actionHash, signL1Action } from '../reference/hyperliquid/hyperliquid.signing';
it('pins Rust schedule action encoding against the independent TS signer', async () => {
  const rows = [];
  for (const mainnet of [false, true]) for (const vault of [null, `0x${'33'.repeat(20)}`]) for (const time of [null, 1_800_000_030_000]) {
    const action = time === null ? { type: 'scheduleCancel' } : { type: 'scheduleCancel', time };
    const nonce = 1_800_000_000_000;
    rows.push({ mainnet, vault, time, nonce, msgpack: ethers.hexlify(encode(action)), hash: actionHash(action, vault, nonce), signature: await signL1Action(new ethers.Wallet(`0x${'11'.repeat(32)}`), action, vault, nonce, mainnet) });
  }
  expect(rows).toEqual(vectors);
});
