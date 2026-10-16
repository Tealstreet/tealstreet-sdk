// Real Dayjs from the same isolated installation as the selected TS compiler.
const fs = require('node:fs'), path = require('node:path');
const {ts, root} = require('./source.cjs');
const tsPath = process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'sdk-rs/target/asterdex-oracle-deps/node_modules/typescript');
const compilerRoot = path.resolve(path.dirname(require.resolve(tsPath)), '..');
if (require(path.join(compilerRoot, 'package.json')).name !== 'typescript') throw Error('Unrecognized TypeScript installation');
const dependencyRoot = path.dirname(compilerRoot);
// Absolute package paths cannot fall back to an ancestor node_modules.
const dayjsPath = path.join(dependencyRoot, ...(process.argv.includes('--missing-package-control') ? ['asterdex-missing-install-control'] : []), 'dayjs');
const dayjs = require(dayjsPath);
dayjs.extend(require(path.join(dayjsPath, 'plugin/utc')));
const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts'),'utf8');
const ast = ts.createSourceFile('clock.ts',source,ts.ScriptTarget.Latest,true);
const cls = ast.statements.find(ts.isClassDeclaration), printer = ts.createPrinter();
const names = ['getCurrentTime','getCurrentTimeMs','getTimeOffset','needsSync'];
const members = names.map(name=>{const n=cls.members.find(n=>n.name?.getText(ast)===name);if(!n)throw Error('Missing '+name);return printer.printNode(ts.EmitHint.Unspecified,n,ast)}).join('\n');
const out={};
new Function('exports','dayjs',ts.transpileModule('export class Clock {'+members+'}',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText)(out,dayjs);
const RealDate = Date;
let now = 0;
global.Date = class extends RealDate {constructor(...args){super(...(args.length ? args : [now]));} static now(){return now;}};
const encode = n => !Number.isFinite(n)||Object.is(n,-0) ? {number:Object.is(n,-0)?'-0':String(n)} : n;
const values = [null,0,-0,0.9,-0.9,1,-1,1000,1001,299999,300000,300001,301000,301001,8640000000000000,-8640000000000000,8640000000000001,Infinity,-Infinity,NaN];
const evidence = [];
try {
 for (const server of values) for (const client of values) for (const instant of values.filter(v=>v!==null)) {
  now=instant;
  const clock=new out.Clock();
  clock.server=server===null?null:dayjs.utc(server);
  clock.client=client===null?null:dayjs.utc(client);
  evidence.push({server:server===null?null:encode(server),client:client===null?null:encode(client),now:encode(now),current:encode(clock.getCurrentTime().valueOf()),ms:encode(clock.getCurrentTimeMs()),offset:encode(clock.getTimeOffset()),needs:clock.needsSync()});
 }
} finally {global.Date=RealDate;}
process.stdout.write(JSON.stringify(evidence));
