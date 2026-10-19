'use strict';
// Primary oracle: the engine's actual Object.defineProperty, with no emulation.
if (process.argv.includes('--array-targets')) {
  const profiles = [[], [-0, , undefined]].map(target => {
    const returned = Object.defineProperty(target, 'x', {});
    const descriptor = Object.getOwnPropertyDescriptor(target, 'x');
    const errors = [
      () => { target.x = 1; },
      () => { delete target.x; },
      () => { Object.defineProperty(target, 'x', {value: 1}); },
    ].map(mutate => {
      try { mutate(); return null; } catch (error) { return error.name; }
    });
    return {
      same: returned === target,
      keys: Object.keys(target),
      flags: [descriptor.writable, descriptor.enumerable, descriptor.configurable],
      own: Object.hasOwn(target, 'x'),
      undefined: descriptor.value === undefined && target.x === undefined,
      length: target.length,
      hole: !Object.hasOwn(target, '1'),
      ownUndefined: Object.hasOwn(target, '2') && target[2] === undefined,
      negativeZero: Object.is(target[0], -0),
      errors,
      wire: JSON.stringify(target),
    };
  });
  process.stdout.write(JSON.stringify(profiles));
  process.exit(0);
}
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
