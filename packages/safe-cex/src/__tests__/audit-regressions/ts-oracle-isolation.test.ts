import { build } from 'esbuild';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { BETA_VENUES } from '../../exchanges/rust/betaVenues';

// The TS venue adapters live only in the test-only replay oracle
// (src/__tests__/reference/<venue>/). The runtime keeps exactly these venue
// files: signing for rust/legacySigning.ts, the index.ts wallet/builder exports,
// and the codecs/types rust/*.ts, transport and accounts import.
const KEPT_VENUE_FILES = [
  'asterdex/asterdex.api.ts',
  'asterdex/asterdex.builder-approval.ts',
  'asterdex/asterdex.builder-fee.ts',
  'asterdex/asterdex.signing.ts',
  'asterdex/asterdex.types.ts',
  'asterdex/asterdex.virtualClock.ts',
  'binance/binance.signing.ts',
  'binance/binance.types.ts',
  'binance/binanceinverse.types.ts',
  'binance/binancepm.types.ts',
  'binance/binancespot.types.ts',
  'bitget/bitget.signing.ts',
  'bitget/bitget.types.ts',
  'bitunix/bitunix.income.ts',
  'bitunix/bitunix.signing.ts',
  'blofin/blofin.income.ts',
  'blofin/blofin.types.ts',
  'bybit/bybit.closed-pnl.ts',
  'bybit/bybit.inverse.ts',
  'bybit/bybit.order-params.ts',
  'bybit/bybit.signing.ts',
  'bybit/bybit.site.ts',
  'bybit/bybit.types.ts',
  'grvt/grvt.builder.ts',
  'grvt/grvt.types.ts',
  'hyperliquid/hyperliquid.agent.ts',
  'hyperliquid/hyperliquid.builderFee.ts',
  'hyperliquid/hyperliquid.cloid.ts',
  'hyperliquid/hyperliquid.constants.ts',
  'hyperliquid/hyperliquid.order-inference.ts',
  'hyperliquid/hyperliquid.types.ts',
  'lighter/lighter.account-tier.ts',
  'lighter/lighter.builder-approval.ts',
  'lighter/lighter.builder-fee.ts',
  'lighter/lighter.types.ts',
  'lighter/lighter.utils.ts',
  'okx/okx.closed-pnl.ts',
  'okx/okx.signing.ts',
  'okx/okx.types.ts',
  'phemex/phemex.types.ts',
  'txflow/txflow.client-id.ts',
];

// Pure helpers tealstreet-next imports from source, though the SDK runtime does not:
// the open-orders Bitget attached TP/SL check and the history-audit Kraken mappers.
const SOURCE_CONSUMER_FILES = ['bitget/bitget.family.ts', 'kraken/kraken.fills.ts', 'kraken/kraken.income.ts'];

// Kept venue code outside the beta families, each for a named runtime consumer.
const NON_BETA_KEPT_FAMILIES: Record<string, string> = {
  grvt: 'index.ts exports the GRVT builder-authorization helpers the web app uses',
  txflow: 'rust/rust.exchange.ts applies the TxFlow client order ID rule',
};

const SRC = resolve(__dirname, '../..');
const EXCHANGES = join(SRC, 'exchanges');
const ALIAS = '@tealstreet/safe-cex/src/';
const SPECIFIER =
  /(?:\bfrom\s*|\bimport\s*\(\s*(?:\/\*[^*]*\*\/\s*)?|\brequire\s*\(\s*|^\s*import\s+)(['"])([^'"\n]+)\1/gm;
const EXTENSIONS = ['', '.ts', '.tsx', '.json', '/index.ts'];

const isTest = (file: string) => /\.(?:test|spec)\.tsx?$/.test(file);
const venueOf = (file: string) => {
  const rel = relative(EXCHANGES, file);
  return !rel.startsWith('..') && rel.includes('/') && !rel.startsWith('rust/') ? rel : undefined;
};

const walk = (dir: string, out: string[] = []): string[] => {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
};

const resolveImport = (from: string, specifier: string) => {
  let base: string;
  if (specifier.startsWith('.')) base = resolve(dirname(from), specifier);
  else if (specifier.startsWith(ALIAS)) base = join(SRC, specifier.slice(ALIAS.length));
  else return undefined;
  for (const extension of EXTENSIONS) {
    const candidate = base + extension;
    if (existsSync(candidate) && statSync(candidate).isFile()) return candidate;
  }
  return undefined;
};

// Every non-test runtime source outside the oracle and the venue directories.
const runtimeRoots = () =>
  walk(SRC).filter(
    (file) =>
      /\.tsx?$/.test(file) &&
      !isTest(file) &&
      !relative(SRC, file).startsWith('__') &&
      !venueOf(file),
  );

// Type and value imports both count: tsc and consumers that compile from source need them.
const reachable = () => {
  const seen = new Set<string>();
  const queue = runtimeRoots();
  while (queue.length) {
    const file = queue.pop()!;
    if (seen.has(file) || !file.endsWith('.ts')) continue;
    seen.add(file);
    for (const match of readFileSync(file, 'utf8').matchAll(SPECIFIER)) {
      const target = resolveImport(file, match[2]);
      if (target && !seen.has(target)) queue.push(target);
    }
  }
  return [...seen].map((file) => relative(SRC, file));
};

const venueFiles = (paths: string[]) =>
  paths
    .filter((path) => path.startsWith('exchanges/') && venueOf(join(SRC, path)))
    .map((path) => path.slice('exchanges/'.length))
    .sort();

describe('the TS venue oracle is unreachable from the SDK runtime', () => {
  it('static type and value imports reach no oracle code and only the reviewed venue files', () => {
    const paths = reachable();
    expect(paths.filter((path) => path.startsWith('__tests__/'))).toEqual([]);
    expect(venueFiles(paths)).toEqual([...KEPT_VENUE_FILES].sort());
  });

  it('production venue directories hold only the reviewed files, and no adapter module', () => {
    const production = venueFiles(walk(EXCHANGES).map((file) => relative(SRC, file))).filter(
      (file) => !file.endsWith('/CLAUDE.md'),
    );
    expect(production).toEqual([...KEPT_VENUE_FILES, ...SOURCE_CONSUMER_FILES].sort());
    expect(production.filter((file) => /\.(?:exchange|ws-private|ws-public)\.ts$/.test(file))).toEqual([]);
  });

  it('kept venue code belongs to a beta family or a named non-beta consumer', () => {
    const families = new Set([...KEPT_VENUE_FILES, ...SOURCE_CONSUMER_FILES].map((file) => file.split('/')[0]));
    const outside = [...families].filter((family) => !(BETA_VENUES as readonly string[]).includes(family)).sort();
    expect(outside).toEqual(Object.keys(NON_BETA_KEPT_FAMILIES).sort());
  });

  it('source-consumer helpers import only shared modules', () => {
    for (const file of SOURCE_CONSUMER_FILES) {
      const path = join(EXCHANGES, file);
      for (const match of readFileSync(path, 'utf8').matchAll(SPECIFIER)) {
        const target = resolveImport(path, match[2]);
        if (!target) continue;
        expect(relative(SRC, target).startsWith('__tests__/'), `${file} -> ${match[2]}`).toBe(false);
        expect(venueOf(target), `${file} -> ${match[2]}`).toBeUndefined();
      }
    }
  });

  it('the bundled SDK entry contains no oracle or adapter module', async () => {
    const result = await build({
      entryPoints: [join(SRC, 'index.ts')],
      bundle: true,
      write: false,
      metafile: true,
      platform: 'node',
      format: 'esm',
      packages: 'external',
      logLevel: 'silent',
      alias: { '@tealstreet/safe-cex/src': SRC },
    });
    const inputs = Object.keys(result.metafile.inputs).map((input) => relative(SRC, resolve(input)));
    expect(inputs.filter((path) => path.startsWith('__tests__/'))).toEqual([]);
    const bundledVenueFiles = venueFiles(inputs);
    expect(bundledVenueFiles.filter((file) => !KEPT_VENUE_FILES.includes(file))).toEqual([]);
  }, 60_000);
});
