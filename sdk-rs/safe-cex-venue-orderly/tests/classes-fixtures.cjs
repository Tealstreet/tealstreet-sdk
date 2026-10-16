'use strict';
const defaults={name:{name:'original'},options:{key:'KEY',secret:'SECRET',extra:{retained:true}},store:{retained:'store'},argument:{symbol:'BTCUSDC',interval:'1m'},callback:{opaque:'callback'},pair:true,invoke:true};
const constructor=[{}, {options:{key:'ed25519:KEY',secret:'ed25519:SECRET'}},{options:{key:'',secret:0}},{options:null},{options:undefined},{options:{key:'キー',secret:'clé'}},{fail:'super'},{fail:'api'},{fail:'rate'},{fail:'public'},{fail:'private'}];
const classCases=[{options:{key:'ed25519:KEY',secret:'ed25519:SECRET'}}, {options:{key:'ed25519:KEY',secret:'SECRET'}},{options:{key:false,secret:null}},{noArgs:true},{fail:'api'},{fail:'private'},{invokeFailure:true}];
const classes={OrderlyExchange:classCases,KodiakFIExchange:classCases,MODEExchange:classCases,SalsaDEXExchange:classCases,TealstreetDEXExchange:classCases,WhatExchange:classCases,WoofiExchange:classCases};
module.exports={defaults,constructor,classes};
