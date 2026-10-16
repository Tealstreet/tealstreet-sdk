'use strict';
const account={account_id:{opaque:'ACCOUNT'},max_leverage:31.5};
const row={symbol:'PERP_BTC_USDC',default_margin_mode:'ISOLATED',id:'raw'};
const mapped={id:'MAPPED',price:23.125};
const defaults={altMapped:{id:"ALT"},altChildren:[{id:"ALT CHILD"}],account,rows:[row,{...row,id:'second'}],mapped,mappedChildren:[mapped,{...mapped,id:'CHILD'}],limitValues:[mapped],algoValues:[{id:'ALGO'}],error:{response:{data:{message:'API MESSAGE',msg:'API MSG'}},message:'ROOT'},mode:null,leverage:1,wire:'original'};
const cases={
 getPositionMode:[{},{mode:'hedged'},{mode:{opaque:'mode'}},{reject:true},{syncFail:true},{afterAwait:{_accountPositionMode:'hedged'}},{mode:0},{repeat:true}],
 getAccount:[{},{account:{account_id:0}},{account:{account_id:''}},{account:{account_id:'A'}},{account:{}},{reject:true},{syncFail:true},{afterAwait:{isDisposed:true}}],
 validateAccount:[{},{reject:true},{syncFail:true},{reject:true,error:{message:'ROOT'}},{reject:true,error:{response:{data:{message:0,msg:false}},message:''}},{reject:true,fail:'log'},{reject:true,afterLog:{response:{data:{msg:'AFTER'}}}},{reject:true,error:{response:{data:{message:{opaque:'LOG'},msg:{opaque:'RETURN'}}},message:'ROOT'}}],
 fetchAndUpdateAccountLeverage:[{},{account:{max_leverage:0}},{account:{max_leverage:'0'}},{account:null},{account:{max_leverage:NaN}},{account:{max_leverage:-0}},{reject:true},{syncFail:true},{afterAwait:{accountLeverage:'changed'}}],
 fetchAndUpdateMarginTypes:[{},{rows:[null,{symbol:0},row,{symbol:'PERP_ETH_USDC',defaultMarginMode:'CROSS'},{symbol:'PERP_SOL_USDC',default_margin_mode:'cross'}]},{rows:null},{rows:{bad:'row'}},{reject:true},{fail:'mapMargin'},{fail:'mapSet'},{append:true}],
 fetchLimitOrders:[{},{mapped:null},{mapped:false},{mapped:undefined},{mapped:0},{rows:[]},{reject:true},{syncFail:true},{fail:'mapLimit'},{reject:true,fail:'log'},{append:true},{afterAwait:{mapLimitOrder:'alternate'}}],
 fetchAlgoOrders:[{},{mappedChildren:[]},{rows:[]},{reject:true},{syncFail:true},{fail:'mapAlgo'},{reject:true,fail:'log'},{append:true},{afterAwait:{mapAlgoOrder:'alternate'}}],
 fetchOrders:[{},{rejectLimit:true},{rejectAlgo:true},{syncLimit:true},{syncAlgo:true},{limitValues:[false,null,0],algoValues:[]},{integrate:true},{integrate:true,reject:true},{integrate:true,fail:'log'}],
};
module.exports={defaults,cases};
