'use strict';
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const sourceRoot = path.resolve(__dirname, '../../../packages/safe-cex/src');
const types = fs.readFileSync(path.join(sourceRoot, 'types.ts'), 'utf8');
const parsed = ts.createSourceFile('types.ts', types, ts.ScriptTarget.Latest, true);
const enumSource = parsed.statements.filter(n => ts.isEnumDeclaration(n) && ['MarginType','PositionMode'].includes(n.name.text)).map(n => n.getText(parsed)).join('\n');
const exportsValue = {};
new Function('exports', ts.transpileModule(enumSource, {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText)(exportsValue);
const configs = [];
for (const [file, name] of [['hyperliquid.exchange.ts','DEFAULT_SYMBOL_CONFIG'], ['hyperliquidspot.exchange.ts','SPOT_SYMBOL_CONFIG']]) {
  const text = fs.readFileSync(path.join(sourceRoot, 'exchanges/hyperliquid', file), 'utf8');
  const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const matches = [];
  function visit(n) { if (ts.isPropertyDeclaration(n) && n.name.getText(ast) === name) matches.push(n.initializer.getText(ast)); ts.forEachChild(n, visit); }
  visit(ast);
  if (matches.length !== 1) throw new Error('Expected maintained frozen configuration: '+name);
  const value = new Function('MarginType','PositionMode', 'return '+matches[0])(exportsValue.MarginType, exportsValue.PositionMode);
  if (!Object.isFrozen(value)) throw new Error('Maintained configuration must remain frozen');
  configs.push({name,value,json:JSON.stringify(value)});
}
const target = {tag:7};
function getter() { return this.tag; }
const writes = [];
function setter(value) { writes.push([this,value]); }
Object.defineProperty(target,'x',{get:getter,set:setter,configurable:true});
const before = Object.getOwnPropertyDescriptor(target,'x');
Object.freeze(target);
const token = {};
target.x = token;
let sameRedefinition = false, changedRefused = false;
try { Object.defineProperty(target,'x',{get:getter}); sameRedefinition = true; } catch (error) { if (!(error instanceof TypeError)) throw error; }
try { Object.defineProperty(target,'x',{get:function other(){}}); } catch (error) { changedRefused = error instanceof TypeError; }
Reflect.deleteProperty(target,'x');
const after = Object.getOwnPropertyDescriptor(target,'x');
const child = Object.create(target, {tag:{value:22}});
Object.preventExtensions(child);
child.x = token;
const inherited = {getterReceiver:child.x===22,setterReceiver:writes[1][0]===child,setterValue:writes[1][1]===token,own:Object.hasOwn(child,'x')};
const reads = [];
const badDescriptor = {};
for (const key of ['enumerable','configurable','value','writable','get','set']) Object.defineProperty(badDescriptor,key,{get(){reads.push(key);return key==='get' ? 7 : undefined;}});
let readError;
try { Object.defineProperty({},'x',badDescriptor); } catch (error) { readError = error.name; }
console.log(JSON.stringify({inherited,readOrder:{reads,error:readError},configs,accessor:{getterSame:before.get===after.get,setterSame:before.set===after.set,enumerable:after.enumerable,configurable:after.configurable,sameRedefinition,changedRefused,ownAfterDelete:Object.hasOwn(target,'x'),setterReceiver:writes[0][0]===target,setterValue:writes[0][1]===token,frozen:Object.isFrozen(target),extensible:Object.isExtensible(target)}}));
