import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource} from './source-loader.mjs';
const base = new URL('../../../packages/safe-cex/src/exchanges/okx/',import.meta.url);
const {PositionMode} = loadSource(new URL('../../types.ts',base));
function members(file,ownerName,names) {
  const url = new URL(file,base);
  const ast = ts.createSourceFile(url.pathname,fs.readFileSync(url,'utf8'),ts.ScriptTarget.Latest,true);
  const owner = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === ownerName);
  return names.map(name => {
    const member = owner.members.find(n => n.name?.getText(ast) === name);
    assert.ok(member,'maintained market policy '+name);
    return member.getText(ast);
  }).join('\n');
}
const source = 'export class Standard {'+members('okx.exchange.ts','OKXExchange',[
  'getInstrumentTypes','isSupportedMarketInstrument','afterMethodCalledInWorker',
])+'}\nexport class EU extends Standard {'+members('okxeu.exchange.ts','OKXEUExchange',[
  'getInstrumentTypes','isSupportedMarketInstrument',
])+'}';
const code=ts.transpileModule(source,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const module={exports:{}};
vm.runInThisContext('(function(module,exports,PositionMode){'+code+'\n})')(module,module.exports,PositionMode);
const input=JSON.parse(fs.readFileSync(0,'utf8'));
assert.equal(input.markets.length,144,'declared market profile floor');
assert.equal(input.workers.length,36,'declared worker profile floor');
const eu=new module.exports.EU();
const markets=input.markets.map(market=>eu.isSupportedMarketInstrument(market));
const workers=input.workers.map(c=>{
  const receiver=new module.exports.Standard();
  receiver._accountPositionMode=c.initial ?? undefined;
  const args=Object.hasOwn(c,'first')?[c.first]:[];
  receiver.afterMethodCalledInWorker({ignored:true},String.fromCharCode(...c.method),...args);
  return receiver._accountPositionMode ?? null;
});
const inherited=Object.create({instType:'FUTURES',ruleType:'xperp',ctType:'linear'});
const inheritedSupported=eu.isSupportedMarketInstrument(inherited);
assert.equal(inheritedSupported,true,'source inherited instrument fields');
const jsonView=JSON.parse(JSON.stringify(inherited));
assert.deepEqual(jsonView,{},'inherited record own JSON view');
process.stdout.write(JSON.stringify({markets,workers,inheritedSupported,jsonView}));
