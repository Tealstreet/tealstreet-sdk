import type {SignRequest,SignResult,Signer,SigningRole} from './host.mjs';
export interface OperationBinding {
 readonly account:string; readonly network:string; readonly stableSubject:string;
 readonly expectedCredential:string; readonly generation:string;
 /** Closed public family metadata; no keys, refs or bearer tokens. */
 readonly metadata:Readonly<Record<string,unknown>>|null;
}
export interface OperationRequest {
 readonly attemptId:number; readonly deadlineMs:number; readonly receipt:SignRequest['receipt'];
 readonly slot:'primary'|'secondary'; readonly role:SigningRole; readonly binding:OperationBinding;
 readonly descriptor:Readonly<Record<string,unknown>>;
}
export interface OperationCodec {
 readonly recipes:readonly string[];
 recipe(descriptor:OperationRequest['descriptor']):string|undefined;
 describe(work:SignRequest,context:{meta:unknown;binding:OperationBinding;deadlineMs:number}):OperationRequest['descriptor'];
 validate(request:OperationRequest,context:{now:number;deadlineMs:number;side:'origin'|'peer';work?:SignRequest}):void;
 verify(result:SignResult,request:OperationRequest & {work?:SignRequest;now:number;side:'origin'|'peer'}):void;
 execute(request:OperationRequest,context:{signal:AbortSignal;now:()=>number;check():number;deadlineMs:number}):SignResult|Promise<SignResult>;
}
export interface OperationCapabilities {readonly v:2;readonly recipes:readonly string[];readonly slots:readonly ('primary'|'secondary')[];}
export interface OperationOptions {
 readonly bindings:Partial<Record<'primary'|'secondary',OperationBinding>>;
 readonly exchangeName:string;readonly accountName:string;readonly codec:OperationCodec;now():number;
}
/** Construct only after authenticated pre-start capability confirmation. */
export function createOperationProvider(options:OperationOptions & {
 capabilities:OperationCapabilities;id?():string;
 send(frame:string,context:{signal:AbortSignal}):Promise<string>|string;
}):Signer;
export function createOperationPeer(options:OperationOptions & {revision():string}):{
 readonly capabilities:OperationCapabilities;
 handle(frame:string,context?:{signal?:AbortSignal}):Promise<string>;
 /** Aborts owned work; never grants a resource-join acknowledgement. */
 dispose():void;
};
