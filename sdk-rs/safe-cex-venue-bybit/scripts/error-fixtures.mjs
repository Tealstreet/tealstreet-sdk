import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
import vm from 'node:vm';
const file = new URL('../../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts', import.meta.url);
const source = ts.createSourceFile('bybit.ts', readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['BYBIT_TRADING_AGREEMENT_RET_CODES', 'numberOrUndefined', 'stringFromUnknown', 'getBybitErrorField', 'getBybitPayloadMessage', 'formatBybitTradingAgreementError'];
const declarations = source.statements.filter(s => ts.isVariableStatement(s) && names.includes(s.declarationList.declarations[0].name.getText(source)));
assert.equal(declarations.length, names.length);
const context = vm.createContext({});
const text = declarations.map(s => s.getText(source).replace(/^export /, '')).join('\n') + '\n globalThis.run = payload => [getBybitPayloadMessage(payload),formatBybitTradingAgreementError(payload,globalThis.exchange)];';
vm.runInContext(ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, context);
const payloads = [null, false, 0, [], {}, 'ordinary error', 'Must agree to trading terms', 'Agreement required for trading this contract'];
const messages = [null, false, 3, '', '  ', '\uFEFF', '\u0085', '.', 'Must SIGN trading terms...\uFEFF', 'AGREEMENT for TRADING THIS CONTRACT', 'ordinary error..', '\uFEFFreason. \uFEFF', '\u0085reason.\u0085'];
const codes = [null, false, [], {}, '', ' ', '\uFEFF', '\u0085', 110123, '110125', '0x1ae2b', '1.10125e5', -0, 1e21, 'Infinity', 'inf', 10003];
for (const code of codes) for (const message of messages) for (const keys of [['retCode','retMsg'],['ret_code','ret_msg'],['code','message'],['code','msg']]) {
  payloads.push({[keys[0]]:code,[keys[1]]:message});
}
for (const first of messages) payloads.push({retCode:110123, retMsg:first, ret_msg:'must agree trading terms', message:'fallback', msg:'last'});
for (const first of codes) payloads.push({retCode:first, ret_code:110123, retMsg:'ordinary error'});
const owner=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BybitExchange');
const method=owner.members.find(n=>n.name?.getText(source)==='formatBybitVenueError');
assert.ok(method?.body, 'live venue formatter is required');
vm.runInContext(ts.transpileModule(`globalThis.format=function(${method.parameters.map(p=>p.getText(source)).join(',')})${method.body.getText(source)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const results=[];
for (const exchange of [null,'bybit','hyrotrader']) for (const payload of payloads) {
  context.exchange=exchange ?? undefined;
  const [message,agreement]=context.run(payload);
  for(const fallback of [null,'transport failed',' \uFEFF ',{message:'fallback body'}, {retMsg:'',message:'blocked'}]) {
    const formatted=context.format.call({name:context.exchange},payload,fallback);
    results.push({payload,exchange,fallback,formatted,message:message??null,agreement:agreement??null});
  }
}
process.stdout.write(JSON.stringify(results));
