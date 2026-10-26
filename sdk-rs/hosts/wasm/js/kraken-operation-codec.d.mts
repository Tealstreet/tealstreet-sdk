import type {OperationCodec} from './hosted-operations.mjs';
export interface KrakenOperationWasm {
 krakenHostedDescriptor(work:string):string;
 krakenHostedValidate(descriptor:string,receipt:string,slot:string,role:string,now:number,deadline:number):void;
 krakenHostedVerify(descriptor:string,result:string,credential:string):void;
}
export interface KrakenOperationExecutor {
 binding(slot:string):string;
 sign(descriptor:string,receipt:string,slot:string,role:string,now:number,deadline:number):string;
}
export function createKrakenOperationCodec(options:{
 wasm:KrakenOperationWasm;identity?:{credential:string;signing:string};
 executor?:KrakenOperationExecutor;slots?:readonly ('primary'|'secondary')[];spot?:boolean;testnet?:boolean;
}):OperationCodec;
