// Private fixture helper; callers execute actual source methods in their corpus.
exports.fixture=(seed,method)=>{
 const input=JSON.parse(seed.raw);
 if(method.endsWith('Balance'))input.portfolios=[];
 else input.portfolios[0].orders[0].legs=[];
 const error={message:'formatter subscriber failed',name:'SubscriberFailure',store_subscriber_ids:[17,29],is_axios_error:true,request:{id:313},stack:'source-stack',response:{data:{msg:'nested-message'}},context:{phase:'formatter'},config:{url:'/sample'},enhanced_message:'enhanced-report',url:'/failure',method:'PATCH',original_error:{sentinel:true},code:'SAMPLE_CODE'};
 return {input,error};
};
