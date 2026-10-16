'use strict';
// Execute the committed oracle's actual primitive binding in memory, with own
// iterator getters. Independent expectations do not come from oracle output.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module');
const filename=path.join(__dirname,'ws-emission-oracle.cjs');
let source=fs.readFileSync(filename,'utf8').split('const diagnostics=')[0];
source+=`
ids=new WeakMap();tags=new WeakMap();nextId=1;calls=[];g={};
let reads=0,step=0;const cached={ [Symbol.iterator](){return{get next(){reads++;return function(){return ++step===1?{done:false,value:1}:{done:true};};}};}};
assert.deepEqual([...bind.__iterate(cached)],[1]);assert.equal(reads,1,'ITERATOR_NEXT_CACHE');
let primitiveSteps=0;assert.throws(()=>[...bind.__iterate({[Symbol.iterator](){return{next(){return !primitiveSteps++?7:{done:true};}};}})],TypeError,'ITERATOR_RESULT_OBJECT');
let returns=0;const closing={ [Symbol.iterator](){return{next(){return{done:false,value:1};},get return(){returns++;return function(){return{};};}};}};
for(const v of bind.__iterate(closing))break;assert.equal(returns,1,'ITERATOR_RETURN_CACHE');
console.log('iterator primitive witnesses0:nextgetter1/resultprimitiveTypeError/returngetter1; stops at three named cases');
`;
const current=new Module(filename,module);current.filename=filename;current.paths=module.paths;current._compile(source,filename);
