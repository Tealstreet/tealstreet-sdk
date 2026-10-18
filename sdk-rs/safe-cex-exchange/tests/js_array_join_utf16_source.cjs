'use strict';
const units = value => Array.from({length:value.length}, (_, index) => value.charCodeAt(index));
const high=String.fromCharCode(0xd800),low=String.fromCharCode(0xdc00);
const cases=[
  ['elements',[undefined,null,high,-0,Infinity],'|'],
  ['separator',['a','b'],low],
  ['default',[high,'x'],undefined],
  ['nested',[[high,null],'tail'],';'],
  ['null separator',['a','b'],null],
  ['empty',[],high],
  ['holes',Array(3),high],
  ['coercion',[{toString(){return high;}}],{toString(){return low;}}],
];
console.log(JSON.stringify(cases.map(([name,value,separator])=>({name,units:units(value.join(separator))}))));
