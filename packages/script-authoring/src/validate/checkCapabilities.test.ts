import { describe, expect, it } from 'vitest';

import { checkCapabilities } from './checkCapabilities';

const script = ({ meta, body }: { meta: string; body: string }) => `
import type { CoreApi, ScriptArgs, ScriptMeta } from '@tealstreet/script-api-core';

export const meta: ScriptMeta = ${meta};

export default async function (args: ScriptArgs, api: CoreApi) {
${body}
}
`;

const CAPS = (writes: string, extra = '') =>
  `{ targets: ['core'], capabilities: { accounts: [], writes: ${writes}, idempotent: false, storage: { enabled: false, quotaBytes: 0 }${extra} } }`;

describe('checkCapabilities — writes', () => {
  it('accepts a declared write', () => {
    const issues = checkCapabilities(
      script({ meta: CAPS(`['placeOrder']`), body: `  await api.exchange.placeOrder({} as never);` }),
    );
    expect(issues).toEqual([]);
  });

  it('flags an undeclared write', () => {
    const issues = checkCapabilities(
      script({ meta: CAPS(`[]`), body: `  await api.exchange.placeOrder({} as never);` }),
    );
    expect(issues).toHaveLength(1);
    expect(issues[0].message).toContain('placeOrder');
    expect(issues[0].line).toBe(7);
  });

  // buildApi validates BOTH capabilities for closePosition, because the
  // adapters synthesize it via placeOrder({reduceOnly:true}). Declaring only
  // closePosition would pass a naive check and throw at runtime.
  it('requires placeOrder alongside closePosition', () => {
    const onlyClose = checkCapabilities(
      script({ meta: CAPS(`['closePosition']`), body: `  await api.exchange.closePosition({ symbol: 'X' });` }),
    );
    expect(onlyClose).toHaveLength(1);
    expect(onlyClose[0].message).toContain('placeOrder');

    const both = checkCapabilities(
      script({
        meta: CAPS(`['closePosition', 'placeOrder']`),
        body: `  await api.exchange.closePosition({ symbol: 'X' });`,
      }),
    );
    expect(both).toEqual([]);
  });

  it('reports each distinct undeclared write once', () => {
    const issues = checkCapabilities(
      script({
        meta: CAPS(`[]`),
        body: `  await api.exchange.cancelOrder('a');\n  await api.exchange.cancelOrder('b');\n  await api.exchange.setLeverage('X', 2);`,
      }),
    );
    expect(issues).toHaveLength(2);
  });
});

describe('checkCapabilities — reads', () => {
  it('permits all reads when reads is omitted', () => {
    expect(
      checkCapabilities(script({ meta: CAPS(`[]`), body: `  await api.getBalance();\n  await api.getOrders();` })),
    ).toEqual([]);
  });

  it('flags a read outside an explicit allowlist', () => {
    const issues = checkCapabilities(
      script({ meta: CAPS(`[]`, `, reads: ['getBalance']`), body: `  await api.getOrders();` }),
    );
    expect(issues).toHaveLength(1);
    expect(issues[0].message).toContain('getOrders');
  });
});

describe('checkCapabilities — runCommand', () => {
  it('flags runCommand when not enabled', () => {
    const issues = checkCapabilities(
      script({ meta: CAPS(`['runCommand']`), body: `  await api.exchange.runCommand('buy');` }),
    );
    expect(issues.some((i) => i.message.includes('runCommand is not enabled'))).toBe(true);
  });

  it('accepts runCommand when enabled and declared', () => {
    const issues = checkCapabilities(
      script({
        meta: CAPS(`['runCommand']`, `, runCommand: { enabled: true, allowedVerbs: ['buy'] }`),
        body: `  await api.exchange.runCommand('buy');`,
      }),
    );
    expect(issues).toEqual([]);
  });
});

// These are the cases that matter most: this check drives an automated repair
// loop, and a false positive sends the model in circles editing a correct
// declaration. Every ambiguity must resolve to silence.
describe('checkCapabilities — refuses to guess', () => {
  it('is silent when meta is built dynamically', () => {
    const source = `
import type { CoreApi, ScriptArgs } from '@tealstreet/script-api-core';
const base = { targets: ['core'] };
export const meta = { ...base, capabilities: { accounts: [], writes: [], idempotent: false, storage: { enabled: false, quotaBytes: 0 } } };
export default async function (args: ScriptArgs, api: CoreApi) {
  await api.exchange.placeOrder({} as never);
}
`;
    expect(checkCapabilities(source)).toEqual([]);
  });

  it('is silent when writes is not a literal array', () => {
    const source = `
import type { CoreApi, ScriptArgs } from '@tealstreet/script-api-core';
const w = ['placeOrder'];
export const meta = { targets: ['core'], capabilities: { accounts: [], writes: w, idempotent: false, storage: { enabled: false, quotaBytes: 0 } } };
export default async function (args: ScriptArgs, api: CoreApi) {
  await api.exchange.cancelOrder('x');
}
`;
    expect(checkCapabilities(source)).toEqual([]);
  });

  it('is silent when there is no meta at all', () => {
    const source = `
import type { CoreApi, ScriptArgs } from '@tealstreet/script-api-core';
export default async function (args: ScriptArgs, api: CoreApi) {
  await api.exchange.placeOrder({} as never);
}
`;
    expect(checkCapabilities(source)).toEqual([]);
  });

  // A nested binding named `api` refers to something else entirely; matching
  // by identifier text alone would emit a phantom finding into the repair loop.
  it('does not flag a nested local that shadows api', () => {
    const source = script({
      meta: CAPS(`[]`),
      body: `  const helper = () => { const api = { exchange: { placeOrder: async (_x: unknown) => {} } }; return api.exchange.placeOrder({}); };\n  await helper();`,
    });
    expect(checkCapabilities(source)).toEqual([]);
  });

  it('is silent when runCommand config is not statically readable', () => {
    const hoisted = `
import type { CoreApi, ScriptArgs } from '@tealstreet/script-api-core';
const RC = { enabled: true, allowedVerbs: ['buy'] };
export const meta = { targets: ['core'], capabilities: { accounts: [], writes: ['runCommand'], idempotent: false, storage: { enabled: false, quotaBytes: 0 }, runCommand: RC } };
export default async function (args: ScriptArgs, api: CoreApi) {
  await api.exchange.runCommand('buy');
}
`;
    expect(checkCapabilities(hoisted)).toEqual([]);
  });

  it('is silent when the runCommand block is spread', () => {
    const spread = `
import type { CoreApi, ScriptArgs } from '@tealstreet/script-api-core';
const base = { allowedVerbs: ['buy'] };
export const meta = { targets: ['core'], capabilities: { accounts: [], writes: ['runCommand'], idempotent: false, storage: { enabled: false, quotaBytes: 0 }, runCommand: { ...base, enabled: true } } };
export default async function (args: ScriptArgs, api: CoreApi) {
  await api.exchange.runCommand('buy');
}
`;
    expect(checkCapabilities(spread)).toEqual([]);
  });

  it('does not chase aliasing (accepted false negative)', () => {
    const issues = checkCapabilities(
      script({ meta: CAPS(`[]`), body: `  const ex = api.exchange;\n  await ex.placeOrder({} as never);` }),
    );
    expect(issues).toEqual([]);
  });
});
