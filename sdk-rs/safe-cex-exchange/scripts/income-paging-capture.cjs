const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { tmpdir } = require('node:os');
const { createRequire } = require('node:module');
const { pathToFileURL } = require('node:url');
const root = path.resolve(__dirname, '../../..');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript');
const runtime = createRequire(path.join(compiler, 'package.json'));
const vitest = path.dirname(runtime.resolve('vitest/package.json'));
const source = path.join(root, 'packages/safe-cex/src/exchanges/income-paging.ts');
const ts = require(compiler);
const ast = ts.createSourceFile(source, fs.readFileSync(source, 'utf8'), ts.ScriptTarget.Latest, true);
const declaration = name => {
  const node = ast.statements.find(node => node.name?.text === name);
  assert(node, `PAGER type declaration ${name}`);
  assert(!node.heritageClauses?.length, `PAGER unexpected heritage ${name}`);
  return node;
};
const members = node => node.members.map(member => {
  assert(ts.isPropertySignature(member), 'PAGER property-only record');
  return [member.name.getText(ast), member.type.getText(ast), Boolean(member.questionToken)];
});
assert.deepEqual(members(declaration('IncomeRangeOptions')), [
  ['startTime', 'number', false], ['endTime', 'number', false],
  ['chunkMs', 'number', false], ['pageLimit', 'number', false],
  ['chunkDelayMs', 'number', true], ['maxPagesPerChunk', 'number', true],
], 'PAGER options source field inventory');
assert.deepEqual(members(declaration('IncomeRangeResult')), [
  ['records', 'IncomeRecord[]', false], ['truncated', 'boolean', false],
  ['exhaustedChunks', 'number', false], ['requests', 'number', false],
], 'PAGER result source field inventory');
const fetcher = declaration('IncomePageFetcher').type;
assert(ts.isFunctionTypeNode(fetcher), 'PAGER fetcher function type');
assert.equal(fetcher.parameters.length, 1, 'PAGER fetcher argument count');
assert.equal(fetcher.type.getText(ast), 'Promise<IncomeRecord[]>', 'PAGER fetcher result type');
assert.deepEqual(members(fetcher.parameters[0].type), [
  ['startTime', 'number', false], ['endTime', 'number', false], ['limit', 'number', false],
], 'PAGER callback source field inventory');

(async () => {
  const { startVitest } = await import(pathToFileURL(path.join(vitest, 'dist/node.js')));
  const directory = fs.mkdtempSync(path.join(tmpdir(), 'income-paging-capture-'));
  const destination = path.join(directory, 'capture.json');
  let ctx;
  try {
  ctx = await startVitest('test', [], {
    root, config: false, watch: false, pool: 'threads', maxWorkers: 1, fileParallelism: false,
    include: ['sdk-rs/safe-cex-exchange/scripts/income-paging-source.test.ts'],
    environment: 'node', reporters: ['dot'],
    env: { SAFE_CEX_INCOME_CAPTURE: destination },
  }, {
    resolve: { alias: { vitest: path.join(vitest, 'dist/index.js') } },
    plugins: [{ name: 'income-paging-private-source-exports', transform(code, id) {
      if (id === source) return code + '\nexport { identify, pageDirection };\n';
    } }],
  });
  assert(ctx, 'income paging real Vitest context required');
  assert.equal(ctx.state.getFiles().length, 1, 'income paging original suite must execute');
  const failed = ctx.state.getFiles().some(file => file.result?.state !== 'pass');
  await ctx.close();
  ctx = undefined;
  assert(!failed, 'income paging actual-source corpus assertions failed');
  assert(fs.existsSync(destination), 'PAGER worker capture must exist');
  const capture = fs.readFileSync(destination, 'utf8');
  JSON.parse(capture);
  await new Promise((resolve, reject) => {
    process.stdout.write('INCOME_PAGING_CAPTURE=' + capture, error => error ? reject(error) : resolve());
  });
  } finally {
    try { await ctx?.close(); }
    finally { fs.rmSync(directory, { recursive: true, force: true }); }
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
