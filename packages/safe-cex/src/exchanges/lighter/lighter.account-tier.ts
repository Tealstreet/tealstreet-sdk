export type LighterAccountTier = 'standard' | 'premium' | 'plus';

export const LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE =
  'Lighter requires Plus or Premium account tier for Tealstreet partner-attributed orders with non-zero integrator fees. Switch this account to Plus or Premium, then retry.';

export const LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE =
  'Lighter requires Plus or Premium account tier before approving Tealstreet partner attribution with non-zero integrator fees. Switch this account to Plus or Premium, then retry.';

export const normalizeLighterAccountTier = (value: unknown): LighterAccountTier | undefined => {
  const normalized = String(value ?? '')
    .trim()
    .toLowerCase();

  if (normalized === 'std') {
    return 'standard';
  }

  return normalized === 'standard' || normalized === 'premium' || normalized === 'plus' ? normalized : undefined;
};

const asRecord = (value: unknown): Record<string, unknown> | undefined =>
  value && typeof value === 'object' ? (value as Record<string, unknown>) : undefined;

export const extractLighterAccountTierFromPayload = (payload: unknown): LighterAccountTier | undefined => {
  const root = asRecord(payload);
  if (!root) {
    return undefined;
  }

  const data = asRecord(root.data);
  const account = asRecord(root.account) ?? asRecord(data?.account);

  return normalizeLighterAccountTier(
    root.user_tier_name ??
      root.userTierName ??
      root.user_tier ??
      root.userTier ??
      root.account_type_name ??
      root.accountTypeName ??
      root.account_type ??
      root.accountType ??
      data?.user_tier_name ??
      data?.userTierName ??
      data?.user_tier ??
      data?.userTier ??
      data?.account_type_name ??
      data?.accountTypeName ??
      data?.account_type ??
      data?.accountType ??
      account?.user_tier_name ??
      account?.userTierName ??
      account?.user_tier ??
      account?.userTier ??
      account?.account_type_name ??
      account?.accountTypeName ??
      account?.account_type ??
      account?.accountType,
  );
};

export const isLighterAccountTierEligibleForPartnerAttribution = (
  tier: LighterAccountTier | undefined,
): boolean => tier === 'premium' || tier === 'plus';

export const hasNonZeroLighterIntegratorFee = (fees: {
  integratorMakerFee?: number;
  integratorTakerFee?: number;
}): boolean => {
  const makerFee = Number(fees.integratorMakerFee ?? 0);
  const takerFee = Number(fees.integratorTakerFee ?? 0);
  return (Number.isFinite(makerFee) && makerFee !== 0) || (Number.isFinite(takerFee) && takerFee !== 0);
};

export const isLighterPartnerAttributionTierRejection = (params: {
  code?: unknown;
  message?: unknown;
}): boolean => {
  const code = String(params.code ?? '').trim();
  if (code === '21520') {
    return true;
  }

  const message = String(params.message ?? '').toLowerCase();
  return (
    message.includes('standard') &&
    (message.includes('integrator') || message.includes('partner attribution')) &&
    (message.includes('plus') || message.includes('premium') || message.includes('account type'))
  );
};
