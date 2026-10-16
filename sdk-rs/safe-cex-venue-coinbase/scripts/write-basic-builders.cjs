const {write,types}=require('./load-starbase.cjs');
const values=[undefined,null,false,true,0,1,[],{},['PostOnly'],{PostOnly:null},'', 'GoodTillCancel','ImmediateOrCancel','FillOrKill','PostOnly','good_til_cancelled','post_only','constructor','toString','__proto__',' ','\uFEFF','\u0085','💰','a\u0000b'];
for(const value of Object.values(types.OrderTimeInForce))values.push(value,{[value]:null},value.toLowerCase(),value+' ');
for(let length=0;length<130;length++)for(const char of ['a','💰','\uFEFF','\u0085'])values.push(char.repeat(length));
const cases=[];
for(const name of ['mapCoinbaseStarbaseTimeInForce','buildCoinbaseStarbaseCancel'])for(const input of values){
 let result;try{result={ok:write[name](input)};}catch(error){result={error:{name:error.name,message:error.message}};}
 cases.push({name,...input===undefined?{}:{input},result});
}
process.stdout.write(JSON.stringify(cases));
