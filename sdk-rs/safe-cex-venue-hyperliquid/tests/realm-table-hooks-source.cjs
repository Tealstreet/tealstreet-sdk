'use strict';
const rows = [];
for (const label of ['get','in','has-own','names','entries','for-in','spread','stringify','Object.keys','Object.values','Object.entries','Object.hasOwn','hasOwnProperty']) {
  let reads = 0, after = false;
  const cause = { [String.fromCharCode(0xd800)]: -0 };
  Object.defineProperty(cause, 'message', { get() { reads++; throw Error('do not inspect'); } });
  const target = new Proxy({}, {
    get(target,key,receiver) { if (key === 'row') throw cause; return Reflect.get(target,key,receiver); },
    has() { throw cause; },
    getOwnPropertyDescriptor() { throw cause; },
    ownKeys() { throw cause; },
  });
  let same = false;
  try {
    switch (label) {
      case 'get': target.row; break;
      case 'in': 'row' in target; break;
      case 'has-own': case 'Object.hasOwn': Object.hasOwn(target,'row'); break;
      case 'names': Object.getOwnPropertyNames(target); break;
      case 'entries': case 'Object.entries': Object.entries(target); break;
      case 'for-in': for (const key in target) void key; break;
      case 'spread': ({...target}); break;
      case 'stringify': JSON.stringify(target); break;
      case 'Object.keys': Object.keys(target); break;
      case 'Object.values': Object.values(target); break;
      case 'hasOwnProperty': Object.prototype.hasOwnProperty.call(target,'row'); break;
      default: throw Error(label);
    }
    after = true;
  } catch (error) { same = error === cause; }
  rows.push({ label, same, after, reads });
}
console.log(JSON.stringify(rows));
