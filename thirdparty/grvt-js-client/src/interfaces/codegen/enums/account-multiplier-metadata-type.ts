export enum EAccountMultiplierMetadataType {
  // ambassador type
  AMBASSADOR = 'AMBASSADOR',
  // referred by ambassador type
  REFERRED_BY_AMBASSADOR = 'REFERRED_BY_AMBASSADOR',
  // admin override with note
  ADMIN = 'ADMIN',
  // campaign boost type
  CAMPAIGN_BOOST = 'CAMPAIGN_BOOST',
}

export const EAccountMultiplierMetadataTypeInt: Record<EAccountMultiplierMetadataType, number> = Object.freeze({
  [EAccountMultiplierMetadataType.AMBASSADOR]: 1,
  [EAccountMultiplierMetadataType.REFERRED_BY_AMBASSADOR]: 2,
  [EAccountMultiplierMetadataType.ADMIN]: 3,
  [EAccountMultiplierMetadataType.CAMPAIGN_BOOST]: 4
})
