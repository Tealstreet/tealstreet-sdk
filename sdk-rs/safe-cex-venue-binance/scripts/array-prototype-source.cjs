// Stock JS property reads: arrays keep their own realm's Array.prototype.
const assert = require('node:assert/strict');
const vm = require('node:vm');
const context = vm.createContext({});
const first = vm.runInContext('({value:[-0], sibling:[], base:Object.prototype})', context);
const foreign = vm.createContext({value:first.value});
const other = vm.runInContext('[]', foreign);
const proto = first.value.__proto__;
const method = first.value.toString;
assert.equal(first.value['absent-stock-key'], undefined);
proto.marker = 13.5;
assert.equal(first.sibling.__proto__.marker, 13.5);
proto.marker = true;
assert.equal(first.sibling.__proto__.marker, true);
method.marker = -0;
const markerBits = Object.is(first.sibling.toString.marker, -0);
method.marker = false;
const profile = {
  arrayPrototype: Array.isArray(proto),
  ownToString: Object.hasOwn(proto, 'toString'),
  distinctFromObject: method !== first.base.toString && proto !== first.base,
  sharedPrototype: first.sibling.__proto__ === proto,
  sharedMethod: first.sibling.toString === method,
  differentRealmPrototype: other.__proto__ !== proto,
  differentRealmMethod: other.toString !== method,
  capturedPrototype: vm.runInContext('value.__proto__', foreign) === proto,
  capturedMethod: vm.runInContext('value.toString', foreign) === method,
  markerBits,
  laterProperty: first.sibling.toString.marker === false,
  elementBits: Object.is(first.value[0], -0),
  rendered: method.call(first.value),
};
for (const [key, value] of Object.entries(profile)) {
  if (key !== 'rendered') assert.equal(value, true, key);
}
assert.equal(profile.rendered, '0');
process.stdout.write(JSON.stringify(profile));
