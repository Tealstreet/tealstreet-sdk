import { build } from 'esbuild';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import ts from 'typescript';
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
const EXTENSIONS = ['', '.ts', '.tsx', '.json', '/index.ts', '/index.tsx'];

// Module specifiers from the TS AST: static imports/exports, import types and literal
// import()/require() arguments, untagged templates included. A computed one is unresolved.
const specifiers = (file: string, text = readFileSync(file, 'utf8')) => {
  const kind = file.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, kind);
  const found: string[] = [];
  const computed: string[] = [];
  const literal = (node?: ts.Node) =>
    node && (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) ? node.text : undefined;
  const visit = (node: ts.Node) => {
    let specifier: string | undefined;
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier) {
      specifier = literal(node.moduleSpecifier);
    } else if (ts.isImportEqualsDeclaration(node) && ts.isExternalModuleReference(node.moduleReference)) {
      specifier = literal(node.moduleReference.expression);
    } else if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument)) {
      specifier = literal(node.argument.literal);
    } else if (
      ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        (ts.isIdentifier(node.expression) && node.expression.text === 'require'))
    ) {
      specifier = literal(node.arguments[0]);
      if (specifier === undefined) computed.push(`${relative(SRC, file)}: ${node.getText(source).slice(0, 80)}`);
    }
    if (specifier !== undefined) found.push(specifier);
    ts.forEachChild(node, visit);
  };
  visit(source);
  return { found, computed };
};

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

// Every non-test runtime source (.ts and .tsx) outside the oracle and the venue directories.
const runtimeRoots = () =>
  walk(SRC).filter(
    (file) =>
      /\.tsx?$/.test(file) &&
      !isTest(file) &&
      !relative(SRC, file).startsWith('__') &&
      !venueOf(file),
  );

// Type and value imports both count: tsc and consumers that compile from source need them.
// `extra` substitutes in-memory source for a file, for the negative controls.
const reachable = (extra: Record<string, string> = {}) => {
  const seen = new Set<string>();
  const computed: string[] = [];
  const queue = [...runtimeRoots(), ...Object.keys(extra)];
  while (queue.length) {
    const file = queue.pop()!;
    if (seen.has(file) || !/\.tsx?$/.test(file)) continue;
    seen.add(file);
    const scan = specifiers(file, extra[file]);
    computed.push(...scan.computed);
    for (const specifier of scan.found) {
      const target = resolveImport(file, specifier);
      if (target && !seen.has(target)) queue.push(target);
    }
  }
  return { paths: [...seen].map((file) => relative(SRC, file)), computed };
};

const venueFiles = (paths: string[]) =>
  paths
    .filter((path) => path.startsWith('exchanges/') && venueOf(join(SRC, path)))
    .map((path) => path.slice('exchanges/'.length))
    .sort();

describe('the TS venue oracle is unreachable from the SDK runtime', () => {
  it('static type and value imports reach no oracle code and only the reviewed venue files', () => {
    const { paths, computed } = reachable();
    expect(paths.filter((path) => path.startsWith('__tests__/'))).toEqual([]);
    expect(venueFiles(paths)).toEqual([...KEPT_VENUE_FILES].sort());
    expect(computed, 'a computed runtime import cannot be checked').toEqual([]);
  });

  it('negative controls: template, quoted and require imports into the oracle are all caught', () => {
    const oracle = '../__tests__/reference/binance/binance.exchange';
    for (const statement of [
      `void import(\`${oracle}\`);`,
      `void import("${oracle}");`,
      `require('${oracle}');`,
      `export * from '${oracle}';`,
      `type Leak = typeof import('${oracle}');`,
    ]) {
      const { found } = specifiers(join(EXCHANGES, 'registry.native.ts'), statement);
      expect(found, statement).toEqual([oracle]);
    }
    // A native-root leak reaches the oracle through the same walk the guard uses.
    const native = join(EXCHANGES, 'registry.native.ts');
    const leaked = reachable({ [native]: `${readFileSync(native, 'utf8')}\nvoid import(\`${oracle}\`);\n` });
    expect(leaked.paths).toContain('__tests__/reference/binance/binance.exchange.ts');
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
      const { found, computed } = specifiers(path);
      expect(computed, file).toEqual([]);
      for (const specifier of found) {
        const target = resolveImport(path, specifier);
        if (!target) continue;
        expect(relative(SRC, target).startsWith('__tests__/'), `${file} -> ${specifier}`).toBe(false);
        expect(venueOf(target), `${file} -> ${specifier}`).toBeUndefined();
      }
    }
  });

  it('the bundled SDK entry contains no oracle or adapter module', async () => {
    const result = await build({
      entryPoints: [join(SRC, 'index.ts'), join(EXCHANGES, 'registry.ts'), join(EXCHANGES, 'registry.native.ts')],
      bundle: true,
      write: false,
      outdir: join(SRC, '.oracle-isolation-bundle'),
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
