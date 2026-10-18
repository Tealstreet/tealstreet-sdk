'use strict';
const vm = require('node:vm');
let calls = 0;
class SourceClass { constructor(value) { ++calls; this.value = value; this.target = new.target; } }
let refusal;
try { SourceClass(); } catch (error) { refusal = error.name; }
const before = calls;
const token = {};
class Derived extends SourceClass {}
const normal = new SourceClass(token);
const explicit = Reflect.construct(SourceClass, [token], Derived);
const bound = SourceClass.bind({ignored:true}, token);
const boundNormal = new bound();
const boundExplicit = Reflect.construct(bound, [], Derived);
class Other { constructor(value) { this.value = value; this.target = new.target; this.other = true; } }
Object.setPrototypeOf(Derived, Other);
const dynamic = new Derived(token);
const foreignRealm = vm.runInNewContext('({Foreign:function Foreign(){},objectPrototype:Object.prototype})');
const foreign = foreignRealm.Foreign;
foreign.prototype = 1;
const fallback = Reflect.construct(SourceClass, [token], foreign);
let invalid;
try { Reflect.construct(SourceClass, [], () => {}); } catch (error) { invalid = error.name; }
console.log(JSON.stringify({refusal,before,normal:[normal.target===SourceClass,normal.value===token,Object.getPrototypeOf(normal)===SourceClass.prototype],explicit:[explicit.target===Derived,explicit.value===token,Object.getPrototypeOf(explicit)===Derived.prototype],bound:[boundNormal.target===SourceClass,boundNormal.value===token,boundExplicit.target===Derived,Object.getPrototypeOf(boundExplicit)===Derived.prototype],dynamic:[dynamic.other,dynamic.target===Derived,dynamic.value===token],fallback:Object.getPrototypeOf(fallback)===foreignRealm.objectPrototype,invalid}));
