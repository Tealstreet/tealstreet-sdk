import { describe, expect, it } from 'vitest';

import {
  enqueueAliasChange,
  syncAliases,
  type AliasChange,
  type AliasSyncHost,
  type AliasSyncState,
} from './aliasSync';

/** An in-memory server with the SQL's semantics: server-assigned revisions. */
const fakeServer = () => {
  let revision = 0;
  const rows = new Map<string, { command: string | null; revision: number }>();
  return {
    rows,
    call: async (changes: AliasChange[], since: number) => {
      for (const c of changes) rows.set(c.name, { command: c.command, revision: ++revision });
      const entries = [...rows.entries()]
        .filter(([, r]) => r.revision > since)
        .sort((a, b) => a[1].revision - b[1].revision)
        .map(([name, r]) => ({ name, command: r.command }));
      return { revision, entries };
    },
  };
};

const device = (server: ReturnType<typeof fakeServer>, aliases: Record<string, string> = {}) => {
  let state: AliasSyncState = { cursor: null, pending: [] };
  const local = { ...aliases };
  const host: AliasSyncHost = {
    owner: 'u1',
    loadState: () => state,
    saveState: (next) => {
      state = next;
    },
    getLocalAliases: () => local,
    applyRemote: (entries) => {
      for (const e of entries) {
        if (e.command === null) delete local[e.name];
        else local[e.name] = e.command;
      }
    },
    call: server.call,
  };
  return {
    local,
    host,
    edit: (name: string, command: string | null) => {
      if (command === null) delete local[name];
      else local[name] = command;
      state = enqueueAliasChange(state, { name, command });
    },
    state: () => state,
  };
};

describe('syncAliases', () => {
  it('uploads aliases that predate syncing, and another device receives them', async () => {
    const server = fakeServer();
    const cli = device(server, { long: 'buy $100 BTCUSDT' });
    const web = device(server);

    await syncAliases(cli.host);
    await syncAliases(web.host);

    expect(web.local).toEqual({ long: 'buy $100 BTCUSDT' });
  });

  it('keeps and reports a pre-sync alias that differs from the account, never overwriting it', async () => {
    const server = fakeServer();
    const first = device(server, { long: 'buy 1' });
    await syncAliases(first.host);
    const second = device(server, { long: 'buy 999', other: 'sell 1' });
    const reports: unknown[] = [];

    await syncAliases({ ...second.host, report: (r) => reports.push(r) });

    expect(second.local.long).toBe('buy 999');
    expect(server.rows.get('long')?.command).toBe('buy 1');
    expect(server.rows.get('other')?.command).toBe('sell 1');
    expect(reports).toEqual([{ conflicts: ['long'], skipped: [], overLimit: [] }]);
  });

  it('drops refused additions at the alias limit instead of retrying them forever', async () => {
    const server = fakeServer();
    const a = device(server);
    await syncAliases(a.host);
    a.edit('x', 'buy 1');
    a.edit('gone', null);
    const reports: Array<{ overLimit: string[] }> = [];

    const ok = await syncAliases({
      ...a.host,
      call: async () => ({ rejected: 'too_many_aliases' as const }),
      report: (r) => reports.push(r),
    });

    expect(ok).toBe(false);
    expect(a.state().pending).toEqual([{ name: 'gone', command: null }]);
    expect(reports[0].overLimit).toEqual(['x']);
  });

  it('propagates a deletion and a later re-creation in server order', async () => {
    const server = fakeServer();
    const a = device(server, { long: 'buy 1' });
    const b = device(server);
    await syncAliases(a.host);
    await syncAliases(b.host);

    b.edit('long', null);
    await syncAliases(b.host);
    await syncAliases(a.host);
    expect(a.local).toEqual({});

    a.edit('long', 'buy 2');
    await syncAliases(a.host);
    await syncAliases(b.host);
    expect(b.local).toEqual({ long: 'buy 2' });
  });

  it('keeps only the last pending change per name and clears the queue once accepted', async () => {
    const server = fakeServer();
    const a = device(server);
    await syncAliases(a.host);
    a.edit('x', 'buy 1');
    a.edit('x', 'buy 2');
    expect(a.state().pending).toEqual([{ name: 'x', command: 'buy 2' }]);

    await syncAliases(a.host);

    expect(a.state().pending).toEqual([]);
    expect(server.rows.get('x')?.command).toBe('buy 2');
  });

  it('keeps the queue and the cursor when a round fails', async () => {
    const server = fakeServer();
    const a = device(server);
    await syncAliases(a.host);
    a.edit('x', 'buy 1');
    const failing: AliasSyncHost = { ...a.host, call: async () => null };

    expect(await syncAliases(failing)).toBe(false);
    expect(a.state().pending).toEqual([{ name: 'x', command: 'buy 1' }]);
  });

  it('starts over when a different user signs in on the same device', async () => {
    const server = fakeServer();
    const a = device(server, { long: 'buy 1' });
    await syncAliases(a.host);
    a.edit('mine', 'buy 2');

    const asOther: AliasSyncHost = { ...a.host, owner: 'u2' };
    const otherServer = fakeServer();
    await syncAliases({ ...asOther, call: otherServer.call });

    expect(otherServer.rows.has('mine')).toBe(false);
    expect(a.state().owner).toBe('u2');
  });

  it('stops re-uploading local aliases after a first sync refused at the limit', async () => {
    const server = fakeServer();
    const a = device(server, { x: 'buy 1' });
    const calls: number[] = [];
    let first = true;
    await syncAliases({
      ...a.host,
      call: async (changes, since) => {
        calls.push(changes.length);
        if (since === 0 && changes.length === 0) return server.call(changes, since);
        if (first) {
          first = false;
          return { rejected: 'too_many_aliases' as const };
        }
        return server.call(changes, since);
      },
    });
    await syncAliases(a.host);

    expect(calls).toEqual([0, 1]);
    expect(server.rows.has('x')).toBe(false);
  });
});
