const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const ts = require('typescript');
const root = path.resolve(__dirname, '../../..');
const names = ['StoreData', 'StoreOptions', 'StoreDataLoaded', 'StoreUpdateIds', 'FetchedLeverage', 'PositionSettings', 'DisplayedMargin'];

test('Store contracts are exact Rust-generated modules without internal mutability directives', () => {
  for (const name of names) {
    const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/generated', `${name}.ts`), 'utf8');
    assert(!source.includes('@ts-mutable'), `${name}: internal mutability directive leaked`);
  }
  const { generateStore } = require('../store-generate.cjs');
  assert.equal(generateStore({ check: true }), true);
});

test('Store contracts preserve readonly, mutable counters, enums and null/absence distinctions', () => {
  const fixture = path.join(root, 'packages/safe-cex/src/generated/__store_contract_check__.ts');
  const imports = names.map((name) => `import type { ${name} } from './${name}';`).join('\n');
  const parityImports = names.map((name) => `import type { ${name} as Public${name} } from '../types';`).join('\n');
  const parity = names
    .filter((name) => name !== 'StoreData')
    .map((name) => `type Parity${name} = Assert<Equal<${name}, Public${name}>>;`)
    .join('\n');
  const source = `${imports}
${parityImports}
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;
${parity}
type OpaqueStoreFields = 'starkConfig' | 'zkLinkConfig' | 'userData';
type StoreKeys = Assert<Equal<keyof StoreData, keyof PublicStoreData>>;
type StoreShape = Assert<Equal<Omit<StoreData, OpaqueStoreFields>, Omit<PublicStoreData, OpaqueStoreFields>>>;
import { MarginType, PositionMode, PositionSide } from '../types';
declare const store: StoreData;
const latency: number | null = store.latency;
const loaded: StoreDataLoaded = store.loaded;
const options: StoreOptions = store.options;
const ids: StoreUpdateIds = store.updateIds;
ids.orders = 0; ids.positions = 0;
const leverage: FetchedLeverage = { buyLeverage: 0, sellLeverage: 0, marginType: MarginType.Cross, positionMode: PositionMode.Hedged };
leverage.isFetching = false;
const margin: DisplayedMargin = { longExposure: 0, shortExposure: 0, totalNotional: 0 };
const unknownMargin: DisplayedMargin = { longExposure: null, shortExposure: 250, totalNotional: null };
const unsafeNotional: number = unknownMargin.totalNotional;
const unsafeLatency: number = store.latency;
loaded.positions = true;
options.isHedged = true;
margin.totalNotional = 1;
const bareMargin: FetchedLeverage['marginType'] = 'cross';
const nullRail: FetchedLeverage = { buyLeverage: null, sellLeverage: 0 };
const missingRail: FetchedLeverage = { buyLeverage: 0 };
const settings: PositionSettings = { id: 'p', symbol: 'BTCUSDT', side: PositionSide.Long, leverage: 7 };
const cached: PositionSettings = store.positionSettings.p;
const fabricatedExposure: PositionSettings = { ...settings, contracts: 0 };
const opaque: string = store.userData;
`;
  const configFile = path.join(root, 'packages/safe-cex/tsconfig.json');
  const config = ts.readConfigFile(configFile, ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configFile));
  const host = ts.createCompilerHost(parsed.options);
  const original = host.getSourceFile.bind(host);
  host.getSourceFile = (file, version, ...args) =>
    file === fixture ? ts.createSourceFile(file, source, version, true) : original(file, version, ...args);
  const program = ts.createProgram([fixture], { ...parsed.options, incremental: false }, host);
  const diagnostics = ts.getPreEmitDiagnostics(program);
  const lineOf = (text) => source.slice(0, source.indexOf(text)).split('\n').length;
  const expected = [
    ['const unsafeNotional:', 2322],
    ['const unsafeLatency:', 2322],
    ['loaded.positions =', 2540],
    ['options.isHedged =', 2540],
    ['margin.totalNotional =', 2540],
    ['const bareMargin:', 2322],
    ['const nullRail:', 2322],
    ['const missingRail:', 2741],
    ['const fabricatedExposure:', 2353],
    ['const opaque:', 2322],
  ];
  assert.deepEqual(
    diagnostics.map((d) => ({
      file: d.file?.fileName,
      code: d.code,
      line: d.file && d.start !== undefined ? d.file.getLineAndCharacterOfPosition(d.start).line + 1 : undefined,
    })),
    expected.map(([text, code]) => ({ file: fixture, code, line: lineOf(text) })),
  );
});
