import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
if (!process.env.SAFE_CEX_TYPESCRIPT_PATH) throw Error('SAFE_CEX_TYPESCRIPT_PATH must name the isolated compiler');
const require = createRequire(import.meta.url);
export const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
export const root = fileURLToPath(new URL('../../../', import.meta.url));
const cache = new Map();
export const boundaries = new Map();
const runtimeRequire = createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-oracle.cjs'));
export function load(relative) {
  const filename=path.resolve(root,relative);
  if(boundaries.has(filename)) return boundaries.get(filename);
  if(cache.has(filename)) return cache.get(filename).exports;
  let source=fs.readFileSync(filename,'utf8');
  if(filename.endsWith('/mexc.types.ts') && process.env.MEXC_TS_CONTROL === 'recv-window') {
    const before='export const RECV_WINDOW = 20_000;';
    if(!source.includes(before)) throw Error('missing TypeScript mutation anchor');
    source=source.replace(before,'export const RECV_WINDOW = 20_001;');
  }
  if(filename.endsWith('/mexc.utils.ts') && process.env.MEXC_TS_CONTROL === 'contract-size') {
    const before='return contractSize > 0 ? contractSize : 1;';
    if(!source.includes(before)) throw Error('missing contract-size mutation anchor');
    source=source.replace(before,'return contractSize > 0 ? contractSize : 2;');
  }
  if(filename.endsWith('/mexc.utils.ts') && process.env.MEXC_TS_CONTROL === 'timestamp-zero') {
    const before='numeric > 0';
    if(!source.includes(before)) throw Error('missing timestamp mutation anchor');
    source=source.replace(before,'numeric >= 0');
  }
  if(filename.endsWith('/mexc.utils.ts') && process.env.MEXC_TS_CONTROL === 'plan-state') {
    const before='const status = normalizeTriggerOrderStatus(raw);';
    if(!source.includes(before)) throw Error('missing plan-state mutation anchor');
    source=source.replace(before,'const status = ORDER_STATUS[toNumber(raw.state ?? raw.status, Number.NaN)] || OrderStatus.Open;');
  }
  if(filename.endsWith('/mexc.utils.ts') && process.env.MEXC_TS_CONTROL === 'execution-side-role') {
    const side='ORDER_SIDE[toNumber(raw.side, Number.NaN)]';
    const maker='toNumber(raw.role) === 1';
    if(!source.includes(side)||!source.includes(maker))throw Error('missing execution identity mutation anchor');
    source=source.replace(side,'ORDER_SIDE[toNumber(raw.role, Number.NaN)]').replace(maker,'toNumber(raw.side) === 1');
  }
  if(filename.endsWith('/mexc.utils.ts') && process.env.MEXC_TS_CONTROL === 'info-alias') {
    const before='info: raw,';
    if(!source.includes(before))throw Error('missing info alias mutation anchor');
    source=source.replace(before,'info: structuredClone(raw),');
  }
  if(filename.endsWith('/mexc.utils.ts')) source += '\nObject.assign(exports,{stepFromScale,normalizePercentage,normalizeOrderbookSide,normalizePlanOrderType,normalizeTriggerOrderStatus,normalizeStopOrderType});';
  if(filename.endsWith('/mexc.api.ts') && process.env.MEXC_TS_CONTROL === 'wire-json') source=source.replaceAll("'Content-Type': 'application/json'", "'Content-Type': 'text/plain'");
  if(filename.endsWith('/mexc.api.ts') && process.env.MEXC_TS_FACTORY !== '1') {
    const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
    const names=new Set(['signMexcRequest','signMexcWsLogin','isMexcPublicEndpoint','normalizeRequestData','serializeMexcParams','getQueryString']);
    source=ast.statements.filter(n =>
      ts.isImportDeclaration(n) ? ['create-hmac','qs','./mexc.types'].includes(n.moduleSpecifier.text) :
      ts.isVariableStatement(n) && n.declarationList.declarations.some(d=>names.has(d.name.getText(ast)))
    ).map(n=>n.getText(ast)).join('\n')+'\nObject.assign(exports,{normalizeRequestData,getQueryString});';
    if(process.env.MEXC_TS_CONTROL==='query-sort') source=source.replace('a.localeCompare(b)', 'b.localeCompare(a)');
    if(process.env.MEXC_TS_CONTROL==='sign-preimage') {
      const before='`${apiKey}${requestTime}${requestParam}`';
      if(!source.includes(before))throw Error('missing signing mutation anchor');
      source=source.replace(before,'`${apiKey}${requestTime}${requestParam}!`');
    }
  }
  if(filename.endsWith('/remoteSigningAdapter.ts')) {
    const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
    source=ast.statements.filter(n=>ts.isVariableStatement(n) && n.declarationList.declarations.some(d=>['SKIP_REMOTE_SIGNING','markSkipRemoteSigning'].includes(d.name.getText(ast)))).map(n=>n.getText(ast)).join('\n');
  }
  const module={exports:{}};cache.set(filename,module);
  const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
  vm.runInThisContext('(function(require,module,exports){'+js+'\n})',{filename})(specifier=>{
    if(boundaries.has(specifier)) return boundaries.get(specifier);
    if(specifier.startsWith('@tealstreet/')) return load('packages/'+specifier.slice('@tealstreet/'.length)+'.ts');
    if(!specifier.startsWith('.')) return runtimeRequire(specifier);
    const target=path.resolve(path.dirname(filename),specifier);
    return load(target.endsWith('.ts')?target:target+'.ts');
  },module,module.exports);
  return module.exports;
}
