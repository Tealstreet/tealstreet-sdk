// The installed lodash helper imported by Bybit Spot order splitting.
'use strict';
const times = require('lodash/times');
const inputs = [
  {}, {count:null}, {count:false}, {count:true}, {count:0}, {count:1}, {count:3},
  {count:3.8}, {count:-1.8}, {count:'3.8'}, {count:'0x3'}, {count:'bad'},
  ...['NaN','Infinity','-Infinity','-0'].map(special=>({special})), {count:9007199254740992},
];
const cases = [];
for (const input of inputs) for (const kind of ['absent','number','property','function']) {
  const count = input.special === undefined ? input.count : Number(input.special);
  const calls = [];
  const iteratee = kind === 'absent' ? undefined : kind === 'number' ? 7 : kind === 'property' ? 'name' : function(index) {
    calls.push({index,arity:arguments.length,undefinedThis:this===undefined});
    return index*10;
  };
  cases.push({input,kind,values:times(count,iteratee),calls});
}
process.stdout.write(JSON.stringify(cases));
