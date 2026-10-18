"use strict";
const assert = require('node:assert/strict');
const moduleValue = require('p-iteration');
const holder = {moduleValue};
const escaped = holder.moduleValue.mapSeries;
delete holder.moduleValue;
escaped([1], function () {
  assert.equal(this, moduleValue);
  return -0;
}, 0).then(values => {
  assert(Object.is(values[0], -0));
  console.log(JSON.stringify(values));
}).catch(error => {console.error(error);process.exitCode=1;});
