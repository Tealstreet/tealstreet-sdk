/** Closed additive v1 join payloads. Validation/signing remains owned by Rust. */
export type JoinProofVenue =
  | 'backpack'
  | 'hyperliquid'
  | 'hyperliquidspot'
  | 'grvt'
  | 'txflow'
  | 'polymarket'
  | 'asterdex'
  | 'orderly'
  | 'kodiakfi'
  | 'mode'
  | 'what'
  | 'woofi'
  | 'lighter'
  | 'lighter-robinhood'
  | 'paradex';
export type JoinProofBytes32 = readonly number[] & { readonly length: 32 };
export interface JoinProofStatement {
  readonly kind: 'tealstreet-admission-v1';
  readonly attempt: string;
  readonly identityDigest: JoinProofBytes32;
  readonly nonce: JoinProofBytes32;
  readonly issuedAtMs: number;
}
export interface JoinProofRequest {
  readonly id: string;
  readonly type: 'join-proof';
  readonly exchangeName: JoinProofVenue;
  readonly accountName: string;
  readonly instanceId: string;
  readonly joinProof: { readonly version: 1; readonly statement: JoinProofStatement };
  readonly config?: never;
  readonly wsAuth?: never;
  readonly error?: never;
}
export type JoinProofResponse =
  | {
      readonly id: string;
      readonly joinProof: { readonly version: 1; readonly signature: string };
      readonly error?: never;
      readonly config?: never;
      readonly wsAuthSignature?: never;
    }
  | {
      readonly id: string;
      readonly error: 'Join proof refused' | 'Join proof unavailable';
      readonly joinProof?: never;
      readonly config?: never;
      readonly wsAuthSignature?: never;
    };
export type JoinProofEnvelope =
  { readonly t: 'sign-req'; readonly d: JoinProofRequest } | { readonly t: 'sign-res'; readonly d: JoinProofResponse };
