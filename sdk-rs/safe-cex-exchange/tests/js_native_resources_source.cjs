'use strict';
const slots = new WeakMap();
const object = {label:'visible'};
slots.set(object, {value:7});
Object.setPrototypeOf(object, {get inherited(){return this.label;}});
const child = Object.assign(Object.create(object), {label:'child'});
Object.freeze(object);
let writeError;
try {object.label='changed';} catch(error) {writeError=error.name;}
slots.get(object).value=8;
console.log(JSON.stringify({keys:Object.keys(object),names:Object.getOwnPropertyNames(object),inherited:object.inherited,child:child.inherited,json:JSON.stringify(object),privateValue:slots.get(object).value,frozen:Object.isFrozen(object),writeError}));
