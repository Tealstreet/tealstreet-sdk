'use strict';
const order={id:'17tail',parentId:'PARENT',symbol:'BTCUSDC',type:'limit',price:23.125,side:'buy',info:{margin_mode:'ISOLATED'}};
const sibling={...order,id:'SIBLING',type:'stop_market',price:37.25};
const positions=[{symbol:'BTCUSDC',marginType:'cross',leverage:5.5},{symbol:'BTCUSDC',marginType:'isolated',leverage:9.5},{symbol:'OTHER',marginType:'cross',leverage:2}];
const defaults={order,orders:[order,{...order,id:'ALGO',type:'stop_market'}],storeOrders:[order,sibling],update:{price:'91.25'},symbol:'BTCUSDC',input:17.5,isolated:true,margin:'cross',market:{limits:{leverage:{min:1.5,max:31.5}}},positions,accountLeverage:5.5,response:{data:{success:true}},error:{response:{data:{message:'API FAILURE'}},message:'ROOT'}};
const cases={
 updateAlgoOrder:[{},{update:{}},{update:{price:undefined}},{order:{...order,parentId:0}},{order:{...order,parentId:'invalid'}},{rejectAt:0},{syncAt:0},{rejectAt:0,fail:'log'}],
 cancelOrders:[{},{orders:[]},{orders:[order],order:{...order,id:'x'},rejectAt:0},{orders:[{...order,type:'stop_market'}],fail:'algo'},{orders:[{...order,id:'0x10',parentId:null}]},{orders:[{...order,id:' -17.9tail'}]},{orders:[order],syncAt:0},{orders:[order],append:true}],
 cancelAlgoOrder:[{},{storeOrders:[order]},{storeOrders:[order,{...sibling,type:'take_profit_market'}]},{order:{...order,parentId:0},storeOrders:[]},{rejectAt:0,error:{response:{data:{message:'order already canceled'}},message:'ROOT'}},{rejectAt:0},{fail:'place'},{afterAwait:{siblingPrice:73.5}},{rejectAt:0,fail:'log'}],
 cancelAllOrders:[{},{disposed:true},{rejectAt:0},{rejectAt:1},{syncAt:0},{rejectAt:0,fail:'log'},{afterAwait:{xhr:'alternate'}}],
 cancelSymbolOrders:[{},{symbol:'USDCBTCUSDC'},{rejectAt:0},{rejectAt:1},{syncAt:0},{rejectAt:0,fail:'log'},{afterAwait:{xhr:'alternate'}}],
 setLeverage:[{},{input:0},{input:100},{input:NaN},{input:'17.5'},{market:null},{margin:'isolated'},{response:{data:{success:1,message:'REJECTED'}}},{response:{data:{success:false}}},{rejectAt:0},{syncAt:0},{fail:'updatePositions'},{afterAwait:{positions:[{symbol:'BTCUSDC',marginType:'cross',leverage:91.5}]}},{disposed:true}],
 setAllLeverage:[{},{input:0},{input:1000},{input:17.49},{input:NaN},{input:'17.5'},{disposed:true},{accountLeverage:18},{accountLeverage:'18'},{response:{data:{success:false}}},{rejectAt:0},{syncAt:0},{rejectAt:0,fail:'log'}],
 changeMarginType:[{},{isolated:false},{symbol:''},{market:null},{response:{data:{success:0,message:'REJECTED'}}},{response:{data:{success:false}}},{rejectAt:0},{syncAt:0},{fail:'mapSet'},{rejectAt:0,fail:'log'}],
};
module.exports={defaults,cases};
