export enum EGrvtEnvironment {
  PRODUCTION = 'PRODUCTION',
  TESTNET = 'TESTNET',
  STAGING = 'STAGING',
  DEV = 'DEV',
}

export interface IGrvtConfig {
  apiKey?: string;
  apiSecret?: string;
  tradingAccountId?: string;
  env: EGrvtEnvironment;
  customAxios?: any; // Optional custom axios instance for HTTP-over-WebSocket (TDG/MDG)
  customEdgeAxios?: any; // Optional custom axios instance for Edge API (authentication)
}

export const CHAIN_IDS: Record<EGrvtEnvironment, number> = {
  [EGrvtEnvironment.DEV]: 327,
  [EGrvtEnvironment.STAGING]: 327,
  [EGrvtEnvironment.TESTNET]: 326,
  [EGrvtEnvironment.PRODUCTION]: 325,
};
