'use strict';
// Primary oracle: the engine's actual Object.defineProperty, with no emulation.
const cases = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const snapshot = object => {
  const descriptor = Object.getOwnPropertyDescriptor(object, 'x');
  return {
    keys: Object.keys(object),
    property: descriptor ? {
      value: descriptor.value === undefined ? 'undefined' : descriptor.value,
      writable: descriptor.writable,
      enumerable: descriptor.enumerable,
      configurable: descriptor.configurable,
    } : null,
  };
};
process.stdout.write(JSON.stringify(cases.map(test => {
  const object = test.function ? function target() {} : {};
  object.keep = 0;
  if (test.existing) object.x = 7;
  const steps = [];
  for (const descriptor of test.descriptors) {
    let error = null;
    try { Object.defineProperty(object, 'x', descriptor); }
    catch (e) { error = e.name; }
    steps.push({error, ...snapshot(object)});
  }
  let writeError = null;
  try { object.x = 99; } catch (e) { writeError = e.name; }
  steps.push({error: writeError, ...snapshot(object)});
  Reflect.deleteProperty(object, 'x');
  steps.push(snapshot(object));
  return steps;
})));
