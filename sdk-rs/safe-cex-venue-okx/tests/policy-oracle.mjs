import assert from 'node:assert/strict';
import fs from 'node:fs';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource} from './source-loader.mjs';
const names = ['findTickerMarkForCurrency','getOKXTriggerPxType','parseOKXTriggerPxType'];
const source = new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts',import.meta.url);
let sourceText=fs.readFileSync(source,'utf8');
if(process.argv.includes('--mutate-inherited-ticker-field')){
 const original=ts.createSourceFile(source.pathname,sourceText,ts.ScriptTarget.Latest,true);
 const target=original.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(n=>n.name.getText(original)==='findTickerMarkForCurrency');
 const literal=target.initializer.parameters[0].type.typeArguments[0];
 assert.ok(ts.isTypeLiteralNode(literal),'mutation requires original direct ticker type literal');
 sourceText=sourceText.slice(0,literal.getStart(original))+'TickerCensus'+sourceText.slice(literal.end);
 sourceText='interface ExtraNumeric { hiddenNumeric: number }\ninterface TickerCensus extends ExtraNumeric { id?: string; symbol?: string; mark?: number }\n'+sourceText;
}
const ast = ts.createSourceFile(source.pathname,sourceText,ts.ScriptTarget.Latest,true);
const declaration = ast.statements.flatMap(node=>ts.isVariableStatement(node)?[...node.declarationList.declarations]:[]).find(node=>node.name.getText(ast)==='findTickerMarkForCurrency');
const input = declaration.initializer.parameters[0].type.typeArguments[0];
assert.ok(ts.isTypeLiteralNode(input),'ticker census requires direct type literal; references and inherited interfaces are unsupported');
assert.deepEqual(input.members.map(member=>[member.name.getText(ast),!!member.questionToken,member.type.kind]),[
  ['id',true,ts.SyntaxKind.StringKeyword],['symbol',true,ts.SyntaxKind.StringKeyword],['mark',true,ts.SyntaxKind.NumberKeyword]
],'optional non-null ticker input contract changed');
const policy = loadSource(source,names,names);
// Assert before JSON.stringify, which would erase the sign and overflow.
for (const mark of [-0, -0.0, Infinity, 1.25]) {
  assert.ok(Object.is(policy.findTickerMarkForCurrency([{id:'BTC-USDT-SWAP',mark}], 'BTC'), mark),
    'source ticker mark retains numeric identity before serialization');
}
const lone=JSON.parse('"\ud800"');
assert.equal(lone.length,1,'surrogate witness must be one UTF-16 code unit');
assert.equal(lone.charCodeAt(0),0xd800,'surrogate witness must retain the lone code unit');
const positiveCurrencies=['BTC','Ω','🚀','\uFEFF'];
assert.equal(positiveCurrencies[3].charCodeAt(0),0xfeff,'BOM positive witness must retain the actual code unit');
for (const currency of [lone,...positiveCurrencies]) {
  assert.equal(policy.findTickerMarkForCurrency([{id:currency+'USD',mark:7}],currency),7,
    'source accepts scalar/astral/whitespace and lone-surrogate identifiers');
}
assert.equal(policy.findTickerMarkForCurrency([{id:'BTCUSD',mark:7,extra:JSON.parse('['.repeat(140)+'0'+']'.repeat(140))}],'BTC'),7,
  'source ignores deep extra fields where bounded Rust ingress refuses');
const cases = [];
for (const currency of ['ETH','SOL','USDT','BTC','']) for (const tickers of [
  [],[{symbol:'ETHFIUSDT',mark:999},{symbol:'ETHUSDT',mark:2}],
  [{id:'ETH-USDT-SWAP',mark:0},{symbol:'ETHUSDT',mark:2}],
  [{id:'ETH-USDT-SWAP'},{symbol:'ETHUSDT',mark:2}],
  [{id:'SOL-USDT-SWAP',mark:4},{symbol:'SOLOUSDT',mark:999}],
  [{symbol:'BTCUSD',mark:5},{id:'BTC-USDT-SWAP',mark:6}],
  [{id:'ETH-USDT-SWAP',symbol:'BTCUSD',mark:-1}],
]) cases.push({op:'mark',currency,tickers,expected:policy.findTickerMarkForCurrency(tickers,currency)});
for (const trigger of [undefined,'index_price','mark_price','last_price']) cases.push({op:'trigger',trigger,expected:policy.getOKXTriggerPxType(trigger)});
for (const trigger of [undefined,'index','mark','last','unknown','','INDEX']) cases.push({op:'parse',trigger,expected:policy.parseOKXTriggerPxType(trigger)});
assert.equal(cases.length,46);
process.stdout.write(JSON.stringify(cases));
