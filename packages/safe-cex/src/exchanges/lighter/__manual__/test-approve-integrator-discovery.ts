import type { AxiosRequestConfig } from 'axios';

import { LighterApiClient, LighterApiError, TransactionClient } from '@tealstreet/lighter-sdk';
import { getCryptoApi, initWasm, isWasmReady } from '@tealstreet/poseidon-crypto';
import { getBaseUrl, RECV_WINDOW } from '../lighter.types';

type RuntimeProcess = {
  env?: Record<string, string | undefined>;
  exitCode?: number;
};

type ProbeConfig = {
  apiPrivateKey?: string;
  accountIndex: bigint;
  apiKeyIndex: number;
  approvalExpiryMs: number;
  baseUrl: string;
  expiredAtMs: number;
  headerAccountIndex: string;
  includeMalformedScenario: boolean;
  integratorAccountIndex: bigint;
  l1Sig?: string;
  l1SigRevocation?: string;
  nonceOverride?: bigint;
  probeReadEndpoints: boolean;
  priceProtection: boolean;
  requestTimeoutMs: number;
  runRevocationScenario: boolean;
  sigBase64: string;
  spotMakerFeeValue: number;
  spotTakerFeeValue: number;
  submitRequests: boolean;
  takerFeeValue: number;
  makerFeeValue: number;
  useRealSigner: boolean;
};

type SendTxProbeResult = {
  code?: number;
  error?: {
    code?: string;
    message: string;
    statusCode?: number;
  };
  name: string;
  success: boolean;
  txInfo: Record<string, unknown>;
  txType: number;
};

const APPROVE_INTEGRATOR_TX_TYPE = 45;
const DAY_MS = 24 * 60 * 60 * 1000;
const DEFAULT_SIG_B64 = 'AQ==';

const getRuntimeProcess = (): RuntimeProcess | undefined => {
  return (globalThis as { process?: RuntimeProcess }).process;
};

const getEnv = (key: string): string | undefined => {
  const runtimeProcess = getRuntimeProcess();
  return runtimeProcess?.env?.[key];
};

const setExitCode = (exitCode: number): void => {
  const runtimeProcess = getRuntimeProcess();
  if (runtimeProcess) {
    runtimeProcess.exitCode = exitCode;
  }
};

const parseBoolean = (value: string | undefined, fallback: boolean): boolean => {
  if (value === undefined) {
    return fallback;
  }

  const normalized = value.trim().toLowerCase();
  if (['1', 'true', 'yes', 'y', 'on'].includes(normalized)) {
    return true;
  }
  if (['0', 'false', 'no', 'n', 'off'].includes(normalized)) {
    return false;
  }

  return fallback;
};

const parseInteger = (name: string, fallback?: number): number => {
  const value = getEnv(name);
  if (!value || value.trim().length === 0) {
    if (fallback !== undefined) {
      return fallback;
    }
    throw new Error(`Missing required env var: ${name}`);
  }

  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed)) {
    throw new Error(`Invalid integer for ${name}: ${value}`);
  }

  return parsed;
};

const parseBigIntValue = (name: string, fallback?: bigint): bigint => {
  const value = getEnv(name);
  if (!value || value.trim().length === 0) {
    if (fallback !== undefined) {
      return fallback;
    }
    throw new Error(`Missing required env var: ${name}`);
  }

  try {
    return BigInt(value);
  } catch {
    throw new Error(`Invalid bigint for ${name}: ${value}`);
  }
};

const normalizeBaseUrl = (url: string): string => {
  return url.endsWith('/') ? url.slice(0, -1) : url;
};

const stringifySafe = (value: unknown): string => {
  return JSON.stringify(
    value,
    (_key, val) => {
      return typeof val === 'bigint' ? val.toString() : val;
    },
    2,
  );
};

const maskMiddle = (value: string): string => {
  if (value.length <= 8) {
    return '*'.repeat(Math.max(value.length, 4));
  }
  return `${value.slice(0, 4)}...${value.slice(-4)}`;
};

const redactHeaders = (headers: unknown): Record<string, unknown> => {
  if (!headers || typeof headers !== 'object') {
    return {};
  }

  const output: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(headers as Record<string, unknown>)) {
    const lowerKey = key.toLowerCase();
    if (lowerKey.includes('authorization') && typeof value === 'string') {
      output[key] = maskMiddle(value);
      continue;
    }
    output[key] = value;
  }
  return output;
};

const formatProbeError = (error: unknown): { code?: string; message: string; statusCode?: number } => {
  if (error instanceof LighterApiError) {
    return {
      code: error.code,
      message: error.message,
      statusCode: error.statusCode,
    };
  }

  if (error instanceof Error) {
    return { message: error.message };
  }

  return { message: String(error) };
};

const shouldAttachLighterAuthHeaders = (url: string): boolean => {
  if (!url) {
    return true;
  }
  return !(url.includes('/public/') || url.includes('/markets') || url.includes('/tickers'));
};

const resolveConfig = (): ProbeConfig => {
  const useTestnet = parseBoolean(getEnv('LIGHTER_TESTNET'), false);
  const now = Date.now();
  const apiPrivateKey = getEnv('LIGHTER_API_PRIVATE_KEY');

  const baseUrl = normalizeBaseUrl(getEnv('LIGHTER_API_URL') ?? getBaseUrl(useTestnet ? 'testnet' : 'livenet'));
  const accountIndex = parseBigIntValue('LIGHTER_ACCOUNT_INDEX');
  const apiKeyIndex = parseInteger('LIGHTER_API_KEY_INDEX', 0);
  const headerAccountIndex = getEnv('LIGHTER_HEADER_ACCOUNT_INDEX') ?? accountIndex.toString();
  const integratorAccountIndex = parseBigIntValue('LIGHTER_INTEGRATOR_ACCOUNT_INDEX', 0n);
  const makerFeeValue = parseInteger('LIGHTER_MAX_PERPS_MAKER_FEE', 300);
  const takerFeeValue = parseInteger('LIGHTER_MAX_PERPS_TAKER_FEE', 300);
  const spotMakerFeeValue = parseInteger('LIGHTER_MAX_SPOT_MAKER_FEE', makerFeeValue);
  const spotTakerFeeValue = parseInteger('LIGHTER_MAX_SPOT_TAKER_FEE', takerFeeValue);
  const approvalExpiryMs = parseInteger('LIGHTER_APPROVAL_EXPIRY_MS', now + 30 * DAY_MS);
  const expiredAtMs = parseInteger('LIGHTER_TX_EXPIRED_AT_MS', now + 10 * 60 * 1000);
  const includeMalformedScenario = parseBoolean(getEnv('LIGHTER_INCLUDE_MALFORMED_SCENARIO'), true);
  const submitRequests = parseBoolean(getEnv('LIGHTER_SUBMIT_DISCOVERY_REQUESTS'), true);
  const probeReadEndpoints = parseBoolean(getEnv('LIGHTER_PROBE_READ_ENDPOINTS'), true);
  const requestTimeoutMs = parseInteger('LIGHTER_REQUEST_TIMEOUT_MS', RECV_WINDOW);
  const priceProtection = parseBoolean(getEnv('LIGHTER_PRICE_PROTECTION'), false);
  const useRealSigner = parseBoolean(getEnv('LIGHTER_USE_REAL_SIGNER'), Boolean(apiPrivateKey));
  const l1Sig = getEnv('LIGHTER_APPROVE_L1_SIG');
  const l1SigRevocation = getEnv('LIGHTER_APPROVE_L1_SIG_REVOCATION');
  const sigBase64 = getEnv('LIGHTER_APPROVE_SIG_B64') ?? DEFAULT_SIG_B64;
  const nonceRaw = getEnv('LIGHTER_NONCE');

  const nonceOverride = nonceRaw ? parseBigIntValue('LIGHTER_NONCE') : undefined;
  const runRevocationScenario = parseBoolean(getEnv('LIGHTER_RUN_REVOCATION_SCENARIO'), true);

  return {
    apiPrivateKey,
    accountIndex,
    apiKeyIndex,
    approvalExpiryMs,
    baseUrl,
    expiredAtMs,
    headerAccountIndex,
    includeMalformedScenario,
    integratorAccountIndex,
    l1Sig,
    l1SigRevocation,
    nonceOverride,
    probeReadEndpoints,
    priceProtection,
    requestTimeoutMs,
    runRevocationScenario,
    sigBase64,
    spotMakerFeeValue,
    spotTakerFeeValue,
    submitRequests,
    takerFeeValue,
    makerFeeValue,
    useRealSigner,
  };
};

const createProbeClient = (config: ProbeConfig): LighterApiClient => {
  const client = new LighterApiClient({
    baseUrl: config.baseUrl,
    timeout: config.requestTimeoutMs,
    debug: false,
    headers: {
      'Content-Type': 'application/json',
    },
  });

  client.addRequestInterceptor((requestConfig) => {
    const configWithTime = requestConfig as AxiosRequestConfig & { __probeStartedAt?: number };
    const now = Date.now();
    configWithTime.__probeStartedAt = now;

    const url = String(configWithTime.url ?? '');
    if (shouldAttachLighterAuthHeaders(url)) {
      configWithTime.headers = {
        ...configWithTime.headers,
        'X-Timestamp': now.toString(),
        'X-Account-Index': config.headerAccountIndex,
      };
    }

    const method = (configWithTime.method ?? 'get').toUpperCase();
    console.log(`\n[approve-probe][request] ${method} ${url}`);
    console.log(`[approve-probe][request.headers] ${stringifySafe(redactHeaders(configWithTime.headers))}`);
    if (configWithTime.params !== undefined) {
      console.log(`[approve-probe][request.params] ${stringifySafe(configWithTime.params)}`);
    }
    if (configWithTime.data !== undefined) {
      console.log(`[approve-probe][request.body] ${String(configWithTime.data)}`);
    }

    return configWithTime;
  });

  client.addResponseInterceptor((response) => {
    const responseConfig = response.config as AxiosRequestConfig & { __probeStartedAt?: number };
    const startedAt = responseConfig.__probeStartedAt;
    const elapsed = typeof startedAt === 'number' ? Date.now() - startedAt : undefined;

    console.log(
      `[approve-probe][response] status=${response.status} url=${String(responseConfig.url ?? '')}${elapsed !== undefined ? ` elapsedMs=${elapsed}` : ''}`,
    );
    console.log(`[approve-probe][response.body] ${stringifySafe(response.data)}`);

    return response;
  });

  client.addErrorInterceptor((error) => {
    const errorInfo = formatProbeError(error);
    console.log(`[approve-probe][error] ${stringifySafe(errorInfo)}`);
    return error;
  });

  return client;
};

const createSigningClient = async (
  client: LighterApiClient,
  config: ProbeConfig,
): Promise<TransactionClient | undefined> => {
  if (!config.useRealSigner) {
    return undefined;
  }
  if (!config.apiPrivateKey || config.apiPrivateKey.trim().length === 0) {
    throw new Error('LIGHTER_USE_REAL_SIGNER=true requires LIGHTER_API_PRIVATE_KEY');
  }

  if (!isWasmReady()) {
    await initWasm();
  }
  const crypto = getCryptoApi();
  const chainId = parseBoolean(getEnv('LIGHTER_TESTNET'), false) ? 300 : 304;

  return new TransactionClient({
    httpClient: client,
    crypto,
    chainId,
    privateKey: config.apiPrivateKey,
    accountIndex: config.accountIndex,
    apiKeyIndex: config.apiKeyIndex,
  });
};

const toSafeNumber = (value: bigint, fieldName: string): number => {
  if (value > BigInt(Number.MAX_SAFE_INTEGER) || value < BigInt(Number.MIN_SAFE_INTEGER)) {
    throw new Error(`${fieldName} exceeds Number safe integer range`);
  }
  return Number(value);
};

const resolveNonce = async (client: LighterApiClient, config: ProbeConfig): Promise<bigint> => {
  if (config.nonceOverride !== undefined) {
    return config.nonceOverride;
  }
  return client.getNextNonce(config.accountIndex, config.apiKeyIndex);
};

const probeReadEndpoints = async (client: LighterApiClient): Promise<void> => {
  const endpoints = ['/api/v1/account', '/api/v1/systemConfig', '/api/v1/system-config'];

  for (const endpoint of endpoints) {
    try {
      const data = await client.get<Record<string, unknown>>(endpoint);
      console.log(`[approve-probe][read-endpoint] ${endpoint} -> ${stringifySafe(data)}`);
    } catch (error) {
      const parsed = formatProbeError(error);
      console.log(`[approve-probe][read-endpoint] ${endpoint} failed -> ${stringifySafe(parsed)}`);
    }
  }
};

const buildApproveIntegratorTxInfo = (config: ProbeConfig, nonce: bigint): Record<string, unknown> => {
  const txInfo: Record<string, unknown> = {
    AccountIndex: toSafeNumber(config.accountIndex, 'AccountIndex'),
    ApiKeyIndex: config.apiKeyIndex,
    IntegratorAccountIndex: toSafeNumber(config.integratorAccountIndex, 'IntegratorAccountIndex'),
    MaxPerpsTakerFee: config.takerFeeValue,
    MaxPerpsMakerFee: config.makerFeeValue,
    MaxSpotTakerFee: config.spotTakerFeeValue,
    MaxSpotMakerFee: config.spotMakerFeeValue,
    ApprovalExpiry: config.approvalExpiryMs,
    ExpiredAt: config.expiredAtMs,
    Nonce: toSafeNumber(nonce, 'Nonce'),
    Sig: config.sigBase64,
  };

  if (config.l1Sig) {
    txInfo.L1Sig = config.l1Sig;
  }

  return txInfo;
};

const buildRevocationTxInfo = (config: ProbeConfig, nonce: bigint): Record<string, unknown> => {
  const txInfo = buildApproveIntegratorTxInfo(config, nonce);
  txInfo.MaxPerpsTakerFee = 0;
  txInfo.MaxPerpsMakerFee = 0;
  txInfo.MaxSpotTakerFee = 0;
  txInfo.MaxSpotMakerFee = 0;
  txInfo.ApprovalExpiry = 0;
  return txInfo;
};

const buildMalformedSnakeCaseTxInfo = (config: ProbeConfig, nonce: bigint): Record<string, unknown> => {
  return {
    account_index: toSafeNumber(config.accountIndex, 'account_index'),
    api_key_index: config.apiKeyIndex,
    integrator_account_index: toSafeNumber(config.integratorAccountIndex, 'integrator_account_index'),
    max_perps_taker_fee: config.takerFeeValue,
    max_perps_maker_fee: config.makerFeeValue,
    max_spot_taker_fee: config.spotTakerFeeValue,
    max_spot_maker_fee: config.spotMakerFeeValue,
    approval_expiry: config.approvalExpiryMs,
    expired_at: config.expiredAtMs,
    nonce: toSafeNumber(nonce, 'nonce'),
    sig: config.sigBase64,
  };
};

const submitScenario = async (
  client: LighterApiClient,
  config: ProbeConfig,
  name: string,
  txType: number,
  txInfo: Record<string, unknown>,
): Promise<SendTxProbeResult> => {
  console.log(`\n[approve-probe][scenario] ${name}`);
  console.log(`[approve-probe][scenario.tx_info] ${stringifySafe(txInfo)}`);

  if (!config.submitRequests) {
    console.log('[approve-probe][scenario] LIGHTER_SUBMIT_DISCOVERY_REQUESTS is false; request was not sent');
    return {
      name,
      success: false,
      txInfo,
      txType,
      error: { message: 'Submission skipped by configuration' },
    };
  }

  const txInfoJson = JSON.stringify(txInfo);
  const body = new URLSearchParams({
    tx_type: String(txType),
    tx_info: txInfoJson,
    ...(config.priceProtection ? { price_protection: 'true' } : {}),
  });

  try {
    const response = await client.post<Record<string, unknown>>('/api/v1/sendTx', body.toString(), {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
    });

    const code = typeof response.code === 'number' ? response.code : undefined;
    const success = code === 200 && typeof response.tx_hash === 'string';

    return {
      code,
      name,
      success,
      txInfo,
      txType,
    };
  } catch (error) {
    return {
      error: formatProbeError(error),
      name,
      success: false,
      txInfo,
      txType,
    };
  }
};

const submitSignedScenario = async (
  client: LighterApiClient,
  config: ProbeConfig,
  signingClient: TransactionClient,
  name: string,
  params: {
    integratorAccountIndex: bigint;
    maxPerpsTakerFee: number;
    maxPerpsMakerFee: number;
    maxSpotTakerFee: number;
    maxSpotMakerFee: number;
    approvalExpiry: bigint;
    nonce?: bigint;
    l1Signature?: string;
  },
): Promise<SendTxProbeResult> => {
  console.log(`\n[approve-probe][scenario] ${name}`);
  console.log(`[approve-probe][scenario.approve_params] ${stringifySafe(params)}`);

  try {
    const txInfo = await signingClient.getApproveIntegratorTransaction(params);
    console.log(
      `[approve-probe][scenario.tx_info_signed] ${stringifySafe({
        type: txInfo.type,
        accountIndex: txInfo.accountIndex.toString(),
        apiKeyIndex: txInfo.apiKeyIndex,
        nonce: txInfo.nonce.toString(),
        expiredAt: txInfo.expiredAt?.toString(),
        approveIntegratorDetails: {
          ...txInfo.approveIntegratorDetails,
          integratorAccountIndex: txInfo.approveIntegratorDetails.integratorAccountIndex.toString(),
          approvalExpiry: txInfo.approveIntegratorDetails.approvalExpiry.toString(),
          expiredAt: txInfo.approveIntegratorDetails.expiredAt?.toString(),
        },
      })}`,
    );

    if (!config.submitRequests) {
      console.log('[approve-probe][scenario] LIGHTER_SUBMIT_DISCOVERY_REQUESTS is false; request was not sent');
      return {
        name,
        success: false,
        txInfo: txInfo.approveIntegratorDetails as unknown as Record<string, unknown>,
        txType: APPROVE_INTEGRATOR_TX_TYPE,
        error: { message: 'Submission skipped by configuration' },
      };
    }

    const txHash = await client.sendRawTx(txInfo);
    console.log(`[approve-probe][scenario.tx_hash] ${txHash}`);
    return {
      name,
      success: true,
      txInfo: txInfo.approveIntegratorDetails as unknown as Record<string, unknown>,
      txType: APPROVE_INTEGRATOR_TX_TYPE,
      code: 200,
    };
  } catch (error) {
    return {
      name,
      success: false,
      txInfo: params as unknown as Record<string, unknown>,
      txType: APPROVE_INTEGRATOR_TX_TYPE,
      error: formatProbeError(error),
    };
  }
};

const main = async (): Promise<void> => {
  const config = resolveConfig();

  console.log('[approve-probe] starting Lighter ApproveIntegrator discovery probe');
  console.log(
    `[approve-probe] config=${stringifySafe({
      accountIndex: config.accountIndex.toString(),
      apiKeyIndex: config.apiKeyIndex,
      baseUrl: config.baseUrl,
      headerAccountIndex: config.headerAccountIndex,
      integratorAccountIndex: config.integratorAccountIndex.toString(),
      makerFeeValue: config.makerFeeValue,
      takerFeeValue: config.takerFeeValue,
      spotMakerFeeValue: config.spotMakerFeeValue,
      spotTakerFeeValue: config.spotTakerFeeValue,
      approvalExpiryMs: config.approvalExpiryMs,
      expiredAtMs: config.expiredAtMs,
      includeMalformedScenario: config.includeMalformedScenario,
      probeReadEndpoints: config.probeReadEndpoints,
      submitRequests: config.submitRequests,
      useRealSigner: config.useRealSigner,
      hasApiPrivateKey: Boolean(config.apiPrivateKey),
      requestTimeoutMs: config.requestTimeoutMs,
      sigBase64: maskMiddle(config.sigBase64),
      l1Sig: config.l1Sig ? maskMiddle(config.l1Sig) : undefined,
      hasL1SigRevocation: Boolean(config.l1SigRevocation),
      runRevocationScenario: config.runRevocationScenario,
      nonceOverride: config.nonceOverride?.toString(),
    })}`,
  );

  const client = createProbeClient(config);

  if (config.probeReadEndpoints) {
    await probeReadEndpoints(client);
  }

  const results: SendTxProbeResult[] = [];

  const signingClient = await createSigningClient(client, config);
  if (signingClient) {
    results.push(
      await submitSignedScenario(client, config, signingClient, 'approve-integrator-pascal-case', {
        integratorAccountIndex: config.integratorAccountIndex,
        maxPerpsTakerFee: config.takerFeeValue,
        maxPerpsMakerFee: config.makerFeeValue,
        maxSpotTakerFee: config.spotTakerFeeValue,
        maxSpotMakerFee: config.spotMakerFeeValue,
        approvalExpiry: BigInt(config.approvalExpiryMs),
        nonce: config.nonceOverride,
        l1Signature: config.l1Sig,
      }),
    );
    if (config.runRevocationScenario) {
      if (config.l1Sig && !config.l1SigRevocation) {
        console.log(
          '[approve-probe] skipping revocation scenario because LIGHTER_APPROVE_L1_SIG_REVOCATION is missing (L1 sigs are payload-specific)',
        );
      } else {
        results.push(
          await submitSignedScenario(client, config, signingClient, 'approve-integrator-revocation', {
            integratorAccountIndex: config.integratorAccountIndex,
            maxPerpsTakerFee: 0,
            maxPerpsMakerFee: 0,
            maxSpotTakerFee: 0,
            maxSpotMakerFee: 0,
            approvalExpiry: 0n,
            nonce: config.nonceOverride !== undefined ? config.nonceOverride + 1n : undefined,
            l1Signature: config.l1SigRevocation,
          }),
        );
      }
    }

    if (config.includeMalformedScenario) {
      console.log('[approve-probe] skipping malformed snake_case scenario in signer mode');
    }
  } else {
    const baseNonce = await resolveNonce(client, config);
    console.log(`[approve-probe] resolved nonce=${baseNonce.toString()}`);

    const scenarios: Array<{ name: string; txInfo: Record<string, unknown> }> = [
      {
        name: 'approve-integrator-pascal-case',
        txInfo: buildApproveIntegratorTxInfo(config, baseNonce),
      },
      {
        name: 'approve-integrator-revocation',
        txInfo: buildRevocationTxInfo(config, baseNonce + 1n),
      },
    ];

    if (config.includeMalformedScenario) {
      scenarios.push({
        name: 'approve-integrator-malformed-snake-case',
        txInfo: buildMalformedSnakeCaseTxInfo(config, baseNonce + 2n),
      });
    }

    for (const scenario of scenarios) {
      const result = await submitScenario(client, config, scenario.name, APPROVE_INTEGRATOR_TX_TYPE, scenario.txInfo);
      results.push(result);
    }
  }

  const successful = results.filter((result) => result.success).length;
  const failed = results.length - successful;

  console.log('\n[approve-probe] summary');
  console.log(`[approve-probe] successful=${successful} failed=${failed} total=${results.length}`);
  console.log(`[approve-probe] result_payload=${stringifySafe(results)}`);

  // Do not fail the script solely on scenario-level API rejections.
  // Discovery runs are expected to produce failures while we map contracts.
};

main().catch((error) => {
  const formatted = formatProbeError(error);
  console.error(`[approve-probe] fatal=${stringifySafe(formatted)}`);
  setExitCode(1);
});
