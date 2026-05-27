import * as Starknet from 'starknet';
interface CheckResult {
    readonly ok: boolean;
    readonly reason?: string;
}
export declare class AccountSupport {
    /** The contract instance of the account. */
    private readonly contract;
    /** The class hash of the account contract. */
    private readonly classHash;
    private signatureFormat;
    constructor(
    /** The contract instance of the account. */
    contract: Starknet.Contract, 
    /** The class hash of the account contract. */
    classHash: string);
    private testClassHash;
    private getFormat;
    check(): Promise<CheckResult>;
    getSeedFromSignature(signature: Starknet.Signature): string;
}
export {};
