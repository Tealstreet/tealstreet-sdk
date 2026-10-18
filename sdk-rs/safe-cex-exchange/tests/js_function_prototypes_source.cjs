'use strict';
const vm = require('node:vm');
function Base() {}
function Derived() {}
const sentinel = {};
const trace = [];
Base.staticValue = sentinel;
Object.defineProperty(Base, 'receiver', {get() { trace.push(this === Derived); return this; }});
Object.defineProperty(Base, 'write', {set(value) { trace.push(this === Derived); this.written = value; }});
Object.defineProperty(Base, 'readonly', {value: sentinel});
Derived.__proto__ = Base;
const inherited = Derived.staticValue === sentinel;
const receiver = Derived.receiver === Derived;
Derived.write = -0;
const written = Object.is(Derived.written, -0);
let readonly, cycle;
try { Derived.readonly = 1; } catch (error) { readonly = error.name; }
try { Base.__proto__ = Derived; } catch (error) { cycle = error.name; }
Derived.__proto__ = 1;
const primitiveIgnored = Object.getPrototypeOf(Derived) === Base;
Object.defineProperty(Derived, '__proto__', {value: sentinel, writable: true, configurable: true});
const ownShadow = Derived.__proto__ === sentinel && Object.getPrototypeOf(Derived) === Base;
delete Derived.__proto__;
const a = vm.runInNewContext('(function A(){})');
const b = vm.runInNewContext('(function B(){})');
const realms = [Object.getPrototypeOf(a) === Object.getPrototypeOf(a), Object.getPrototypeOf(a) !== Object.getPrototypeOf(b), a.call !== b.call];
const call = Derived.call, bind = Derived.bind;
const methods = [call === Base.call, call === Derived.call, bind === Base.bind, bind === Derived.bind, Object.getPrototypeOf(call) === Function.prototype];
function target(value) { return [this, value]; }
target.__proto__ = Base;
const bound = target.bind(sentinel, -0);
const result = bound();
const binding = [Object.getPrototypeOf(bound) === Base, bound.staticValue === sentinel, result[0] === sentinel, Object.is(result[1], -0), bound.name];
const unknownKey = Derived['\ud800'] === undefined;
const empty = Function.prototype.call(null) === undefined;
function Detached() {}
Detached.__proto__ = null;
let coercion;
try { String(Detached); } catch (error) { coercion = error.name; }
const detached = [Detached.call === undefined, Detached.__proto__ === undefined, coercion];
const metadata = vm.runInNewContext(`
  [Object.prototype, Object.prototype.toString, Function.prototype, Function, Function.prototype.call].map(value => {
    Object.defineProperty(value, '__proto__', {value:9,configurable:true});
    const parent = Object.getPrototypeOf(value);
    return parent === null ? 'null' : parent === Function.prototype ? 'Function.prototype' : parent === Object.prototype ? 'Object.prototype' : 'unexpected';
  })
`);
console.log(JSON.stringify({inherited,receiver,written,readonly,cycle,primitiveIgnored,ownShadow,trace,realms,methods,binding,unknownKey,empty,detached,metadata}));
