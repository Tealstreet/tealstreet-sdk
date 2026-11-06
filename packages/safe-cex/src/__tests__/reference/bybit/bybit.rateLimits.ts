/**
 * Bybit API Rate Limit Rules For VIPs
 * Source: https://bybit-exchange.github.io/docs/v5/rate-limit (bottom table)
 * All values represent requests per second
 */

export enum BybitVipLevel {
  NoVIP = 'No VIP',
  VIP1 = 'VIP-1',
  VIP2 = 'VIP-2',
  VIP3 = 'VIP-3',
  VIP4 = 'VIP-4',
  VIP5 = 'VIP-5',
  VIPSupreme = 'VIP-Supreme',
  PRO1 = 'PRO-1',
  PRO2 = 'PRO-2',
  PRO3 = 'PRO-3',
  PRO4 = 'PRO-4',
  PRO5 = 'PRO-5',
  PRO6 = 'PRO-6',
}

export enum BybitProductType {
  Futures = 'futures',
  Option = 'option',
  Spot = 'spot',
}

export enum BybitAccountType {
  ClassicAndUTA = 'classicAndUTA',
  UTAPro = 'utaPro',
}

export interface BybitRateLimitRow {
  level: BybitVipLevel;
  [BybitProductType.Futures]: {
    [BybitAccountType.ClassicAndUTA]: number;
    [BybitAccountType.UTAPro]: number;
  };
  [BybitProductType.Option]: {
    [BybitAccountType.ClassicAndUTA]: number;
    [BybitAccountType.UTAPro]: number;
  };
  [BybitProductType.Spot]: {
    [BybitAccountType.ClassicAndUTA]: number;
    [BybitAccountType.UTAPro]: number;
  };
}

export const BYBIT_RATE_LIMIT_TABLE: BybitRateLimitRow[] = [
  {
    level: BybitVipLevel.NoVIP,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 10,
      [BybitAccountType.UTAPro]: 10,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 10,
      [BybitAccountType.UTAPro]: 10,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 20,
      [BybitAccountType.UTAPro]: 20,
    },
  },
  {
    level: BybitVipLevel.VIP1,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 20,
      [BybitAccountType.UTAPro]: 20,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 20,
      [BybitAccountType.UTAPro]: 20,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 25,
      [BybitAccountType.UTAPro]: 25,
    },
  },
  {
    level: BybitVipLevel.VIP2,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 40,
      [BybitAccountType.UTAPro]: 40,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 40,
      [BybitAccountType.UTAPro]: 40,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 30,
      [BybitAccountType.UTAPro]: 30,
    },
  },
  {
    level: BybitVipLevel.VIP3,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 60,
      [BybitAccountType.UTAPro]: 60,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 60,
      [BybitAccountType.UTAPro]: 60,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 40,
      [BybitAccountType.UTAPro]: 40,
    },
  },
  {
    level: BybitVipLevel.VIP4,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 60,
      [BybitAccountType.UTAPro]: 60,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 60,
      [BybitAccountType.UTAPro]: 60,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 40,
      [BybitAccountType.UTAPro]: 40,
    },
  },
  {
    level: BybitVipLevel.VIP5,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 60,
      [BybitAccountType.UTAPro]: 60,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 60,
      [BybitAccountType.UTAPro]: 60,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 40,
      [BybitAccountType.UTAPro]: 40,
    },
  },
  {
    level: BybitVipLevel.VIPSupreme,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 60,
      [BybitAccountType.UTAPro]: 60,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 60,
      [BybitAccountType.UTAPro]: 60,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 40,
      [BybitAccountType.UTAPro]: 40,
    },
  },
  {
    level: BybitVipLevel.PRO1,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 100,
      [BybitAccountType.UTAPro]: 150,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 100,
      [BybitAccountType.UTAPro]: 150,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 50,
      [BybitAccountType.UTAPro]: 150,
    },
  },
  {
    level: BybitVipLevel.PRO2,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 150,
      [BybitAccountType.UTAPro]: 200,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 150,
      [BybitAccountType.UTAPro]: 200,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 75,
      [BybitAccountType.UTAPro]: 200,
    },
  },
  {
    level: BybitVipLevel.PRO3,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 200,
      [BybitAccountType.UTAPro]: 250,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 200,
      [BybitAccountType.UTAPro]: 250,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 100,
      [BybitAccountType.UTAPro]: 250,
    },
  },
  {
    level: BybitVipLevel.PRO4,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 200,
      [BybitAccountType.UTAPro]: 300,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 200,
      [BybitAccountType.UTAPro]: 300,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 100,
      [BybitAccountType.UTAPro]: 300,
    },
  },
  {
    level: BybitVipLevel.PRO5,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 200,
      [BybitAccountType.UTAPro]: 300,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 200,
      [BybitAccountType.UTAPro]: 300,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 100,
      [BybitAccountType.UTAPro]: 300,
    },
  },
  {
    level: BybitVipLevel.PRO6,
    [BybitProductType.Futures]: {
      [BybitAccountType.ClassicAndUTA]: 200,
      [BybitAccountType.UTAPro]: 300,
    },
    [BybitProductType.Option]: {
      [BybitAccountType.ClassicAndUTA]: 200,
      [BybitAccountType.UTAPro]: 300,
    },
    [BybitProductType.Spot]: {
      [BybitAccountType.ClassicAndUTA]: 100,
      [BybitAccountType.UTAPro]: 300,
    },
  },
];
