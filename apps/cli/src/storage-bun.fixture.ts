// Invoked only by cross-user-isolation tests, always with a throwaway profile.
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { Database } from 'bun:sqlite';

if (!process.env.TEALSTREET_CONFIG_DIR?.includes('cli-owner-'))
  throw new Error('fixture requires an isolated test profile');
globalThis.fetch = Object.assign(
  async () => {
    throw new Error('network forbidden in storage fixture');
  },
  {
    preconnect() {
      throw new Error('network forbidden in storage fixture');
    },
  },
);
const mode = process.argv[2];
const root = process.env.TEALSTREET_CONFIG_DIR;
assert.ok(root);
if (mode === 'legacy-recording') {
  mkdirSync(root, { recursive: true });
  writeFileSync(
    join(root, 'config.json'),
    JSON.stringify({ accounts: [], auth: { userId: 'A', accessToken: 'fixture' } }),
  );
  const { runMigrations } = await import('./record/migrations');
  const legacy = new Database(join(root, 'market-data.db'));
  legacy.exec('PRAGMA journal_mode=WAL');
  runMigrations(legacy);
  legacy.exec(
    "INSERT INTO orderbook_snapshots (exchange, symbol, timestamp, bids, asks) VALUES ('fixture','TEST',1,'[]','[]')",
  );
  // Keep the writer open: the migration must include its committed WAL pages.
  const db = await import('./record/database');
  assert.equal(db.countOrderbookSnapshots('fixture', 'TEST'), 1);
  assert.notEqual(db.getDatabasePath(), join(root, 'market-data.db'));
  db.closeDatabase();
  legacy.close();
} else {
  const config = await import('./config');
  function login(userId: string) {
    const c = config.loadConfig();
    c.auth = { userId, accessToken: 'fixture' };
    config.saveConfig(c);
  }
  if (mode !== 'probe') login(process.env.FIXTURE_OWNER || 'A');
  if (mode === 'seed') {
    config.addAccount({ name: 'Main', exchange: 'bybit', apiKey: 'fixture-A', apiSecret: 'fixture-secret' });
    const tasks = await import('./tasks-store');
    tasks.upsertTask({
      id: 'fixture',
      type: 'fixture',
      status: 'paused',
      createdAt: '2026-01-01',
      params: {},
      progress: {},
    });
  } else if (mode === 'probe') {
    const tasks = await import('./tasks-store');
    console.log(JSON.stringify({ accounts: config.listAccounts().length, tasks: tasks.loadTasks().length }));
  } else if (mode === 'recording') {
    const db = await import('./record/database');
    db.upsertRecording({
      dataType: 'orderbook',
      exchange: 'bybit',
      symbol: 'TEST',
      intervalMs: 1000,
      retentionDays: 1,
      config: null,
      active: true,
    });
    db.insertOrderbookSnapshot('bybit', 'TEST', 1, [[1, 2]], [[3, 4]]);
    const aPath = db.getDatabasePath();
    login('B');
    assert.equal(db.getRecordings().length, 0);
    assert.equal(db.countOrderbookSnapshots('bybit', 'TEST'), 0);
    assert.notEqual(db.getDatabasePath(), aPath);
    db.deactivateRecording('orderbook', 'bybit', 'TEST');
    login('A');
    assert.equal(db.getRecordings()[0]?.active, true);
    assert.equal(db.countOrderbookSnapshots('bybit', 'TEST'), 1);
    db.closeDatabase();
  } else if (mode === 'scripts') {
    const scripts = await import('./scripts-store');
    mkdirSync(scripts.getScriptsDir(), { recursive: true });
    writeFileSync(
      join(scripts.getScriptsDir(), 'fixtureA.ts'),
      'export const meta = { targets: ["cli"], capabilities: {} }; export default async function() { throw new Error("never run"); }',
    );
    await scripts.loadScripts();
    assert.ok(scripts.lookupScript('fixtureA'));
    login('B');
    assert.equal(scripts.lookupScript('fixtureA'), undefined);
    await scripts.loadScripts();
    assert.equal(scripts.lookupScript('fixtureA'), undefined);
    login('A');
    await scripts.loadScripts();
    assert.ok(scripts.lookupScript('fixtureA'));
  } else if (mode === 'log') {
    const { logger } = await import('./logger');
    logger.info('fixture-A-marker');
    const aPath = logger.getLogPath();
    login('B');
    logger.info('fixture-B-marker');
    assert.notEqual(logger.getLogPath(), aPath);
    assert.ok(!readFileSync(logger.getLogPath(), 'utf8').includes('fixture-A-marker'));
    assert.ok(readFileSync(aPath, 'utf8').includes('fixture-A-marker'));
    assert.ok(!existsSync(join(root, 'cli.log')));
  }
}
console.log('PASS');
process.exit(0);
