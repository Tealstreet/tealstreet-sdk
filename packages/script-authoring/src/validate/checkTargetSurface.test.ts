import { describe, expect, it } from 'vitest';

import { checkTargetSurface } from './checkTargetSurface';

const wrap = (targets: readonly string[], body: string) => `
import type { CoreApi, ScriptArgs, ScriptMeta } from '@tealstreet/script-api-core';

export const meta: ScriptMeta = {
  targets: ${JSON.stringify(targets)},
  capabilities: {
    accounts: [],
    reads: ['getTicker', 'getOrderbook', 'getOrders', 'getPositions', 'getBalance'],
    writes: ['placeOrder'],
    idempotent: true,
    storage: { enabled: false, quotaBytes: 0 },
  },
};

export default async function (args: ScriptArgs, api: CoreApi) {
${body}
}
`;

describe('checkTargetSurface', () => {
  it('allows shared read-only market scripts across web, mobile and CLI', () => {
    const issues = checkTargetSurface(
      wrap(['core', 'web', 'mobile', 'cli'], '  const ticker = await api.getTicker("BTCUSDT");\n  api.io.return(ticker);'),
    );
    expect(issues).toEqual([]);
  });

  it('rejects web-only helpers when mobile or CLI are targeted', () => {
    const issues = checkTargetSurface(wrap(['core', 'web', 'mobile', 'cli'], '  const price = api.ui.getClick();'));
    expect(issues).toHaveLength(1);
    expect(issues[0].message).toContain('api.ui.getClick');
    expect(issues[0].message).toContain("'mobile'");
    expect(issues[0].message).toContain("'cli'");
    expect(issues[0].message).toContain('api.getTicker');
  });

  it('rejects writes for cross-target marketplace scripts', () => {
    const mobileIssues = checkTargetSurface(
      wrap(['core', 'web', 'mobile', 'cli'], '  await api.exchange.placeOrder({} as never);'),
    );
    expect(mobileIssues).toHaveLength(1);
    expect(mobileIssues[0].message).toContain("'web'");
    expect(mobileIssues[0].message).toContain("'mobile'");
    expect(mobileIssues[0].message).toContain("'cli'");
  });

  it('handles aliases and destructuring from the api root', () => {
    const aliased = checkTargetSurface(
      wrap(
        ['core', 'mobile'],
        '  const ex = api.exchange;\n  await ex.placeOrder({} as never);\n  const { ui } = api;\n  ui.getOrderSize();',
      ),
    );
    expect(aliased.map((issue) => issue.message)).toEqual(
      expect.arrayContaining([expect.stringContaining('api.exchange.placeOrder'), expect.stringContaining('api.ui')]),
    );
  });

  it('rejects disallowed API paths inside helper functions', () => {
    const source = `
import type { CoreApi, ScriptArgs, ScriptMeta } from '@tealstreet/script-api-core';

export const meta: ScriptMeta = {
  targets: ['core', 'web', 'mobile', 'cli'],
  capabilities: {
    accounts: [],
    reads: ['getTicker', 'getOrderbook'],
    writes: ['placeOrder'],
    idempotent: true,
    storage: { enabled: false, quotaBytes: 0 },
  },
};

const placeFromHelper = async (api: CoreApi) => {
  const ex = api.exchange;
  await ex.placeOrder({} as never);
};

export default async function (args: ScriptArgs, api: CoreApi) {
  await placeFromHelper(api);
}
`;

    const issues = checkTargetSurface(source);
    expect(issues).toHaveLength(1);
    expect(issues[0].message).toContain('api.exchange.placeOrder');
    expect(issues[0].message).toContain("'web'");
    expect(issues[0].message).toContain("'mobile'");
    expect(issues[0].message).toContain("'cli'");
  });

  it('treats core-only declarations as the portable intersection', () => {
    const issues = checkTargetSurface(wrap(['core'], '  await api.exchange.placeOrder({} as never);'));
    expect(issues).toHaveLength(1);
    expect(issues[0].message).toContain("'web'");
    expect(issues[0].message).toContain("'mobile'");
    expect(issues[0].message).toContain("'cli'");
  });
});
