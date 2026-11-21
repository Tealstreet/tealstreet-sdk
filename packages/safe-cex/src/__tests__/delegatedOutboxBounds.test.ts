import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { DELEGATED_ACKNOWLEDGED_MAX_AGE_MS, DELEGATED_RECEIPT_MAX_AGE_MS } from '../delegatedOutbox';

// AGT-125: the outbox's handoff windows are derived from limits the box enforces in Rust.
// If one of these moves, re-derive the windows rather than editing the pattern.
const rust = (path: string) => readFileSync(resolve(__dirname, '../../../../sdk-rs', path), 'utf8');
const number = (source: string, pattern: RegExp) => {
  const match = pattern.exec(source);
  if (!match) throw new Error(`Box limit not found: ${pattern}`);
  return Number(match[1].replaceAll('_', ''));
};

describe('DELEGATED outbox windows cover the box limits', () => {
  const contract = rust('hosts/server/src/order_contract.rs');
  const admissionAge = number(contract, /if age >= ([\d_]+) \{\s*Err\("not sent: expired"\)/);
  const futureTolerance = number(contract, /created\.saturating_sub\(now\) > ([\d_]+)/);
  const commandWait = number(
    rust('engine/src/request_context_entry.rs'),
    /pub const DEFAULT_COMMAND_WAIT_MS: u32 = ([\d_]+);/,
  );
  const venueWrite = number(
    rust('venues/hyperliquid/src/trading/custody.rs'),
    /fn call\(\) -> SerialCall \{[^}]*timeout_ms: ([\d_]+),/,
  );

  it('waits for a receipt as long as the box can still admit the key, plus transit', () => {
    expect([admissionAge, futureTolerance]).toEqual([10_000, 5_000]);
    expect(DELEGATED_RECEIPT_MAX_AGE_MS).toBeGreaterThanOrEqual(admissionAge + futureTolerance + 5_000);
  });

  it('waits for an ACKed outcome as long as the box can still execute it', () => {
    expect([commandWait, venueWrite]).toEqual([120_000, 20_000]);
    expect(DELEGATED_ACKNOWLEDGED_MAX_AGE_MS).toBeGreaterThanOrEqual(
      DELEGATED_RECEIPT_MAX_AGE_MS + commandWait + venueWrite,
    );
  });
});
