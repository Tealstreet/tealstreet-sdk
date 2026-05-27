export const BASE_URL = 'https://api1.btloginc.com:9081';

export const ENDPOINTS = {
  LOGIN: '/v1/user/login',
  KEEPALIVE: '/v1/user/keepalive',
  SYMBOL_LIST: '/v1/config/symbollist',
  ACCOUNT_GROUP: '/v1/config/accountgroup',
  ACCOUNT_INFO: '/v1/account/account',
  POSITION_LIST: '/v1/account/positionlist',
  OPEN_POSITION: '/v1/account/openposition',
  CLOSE_POSITION: '/v1/account/closeposition',
  UPDATE_POSITION: '/v1/account/updateposition',
  OPEN_PENDING: '/v1/account/openpending',
  UPDATE_PENDING: '/v1/account/updatepending',
  CANCEL_PENDING: '/v1/account/cancelpending',
  ORDER_LIST: '/v1/account/orderList',
  TRADE_REPORT: '/v1/user/getTradeReport',
  PROFIT_REPORT: '/v1/user/getProfitReport',
  REPORT_DEAL: '/v1/user/getReportDeal',
} as const;

export type BTCCLoginResponse = {
  code: number;
  msg: string;
  token: string;
  server_time: number;
  account: BTCCAccountRaw;
};

export type BTCCAccountRaw = {
  id: number;
  userid: number;
  account_groupid: number;
  account_no: string;
  balance: number;
  equity: number;
  margin: number;
  free_margin: number;
  positions: number;
  bonus?: number;
  status: number;
  margin_level?: number;
  margin_stopout?: number;
  [key: string]: any;
};

export type BTCCSymbolRaw = {
  id: number;
  uuid: number;
  name: string;
  enable: boolean;
  digits: number;
  volumes_min: number;
  volumes_step: number;
  volumes_max: number;
  base_currency: string;
  profit_currency: string;
  margin_currency: string;
  contract_size?: number;
  margin_initial?: number;
  maxstop_level?: number;
  stop_level?: number;
  multiple_list?: string;
  position_volumes_max?: number;
  leverage_init?: number;
  leverage_max?: number;
  [key: string]: any;
};

export type BTCCSymbolListResponse = {
  code: number;
  msg: string;
  symbols: BTCCSymbolRaw[];
};

export type BTCCAccountInfoResponse = {
  code: number;
  msg: string;
  account: BTCCAccountRaw;
};

export type BTCCPositionRaw = {
  id: number;
  accountid: number;
  symbol: string;
  direction: number;
  status: number;
  volume: number;
  init_volume?: number;
  exec_volume: number;
  open_price: number;
  execute_price?: number;
  close_price: number;
  exec_time: number;
  open_time: number;
  close_time: number;
  profit: number;
  exec_profit?: number;
  swap: number;
  exec_swap?: number;
  commission?: number;
  exec_commission?: number;
  take_profit?: number;
  stop_loss?: number;
  multiple?: number;
  open_margin?: number;
  deal_id?: number;
  pos_type?: number;
  [key: string]: any;
};

export type BTCCPositionListResponse = {
  code: number;
  msg: string;
  total?: number;
  positions: BTCCPositionRaw[];
};

export type BTCCOrderRaw = {
  id: number;
  accountid: number;
  symbol: string;
  direction: number;
  type: number;
  status: number;
  request_volume: number;
  request_price: number;
  execute_volume: number;
  execute_price: number;
  execute_time: number;
  stop_loss?: number;
  take_profit?: number;
  expire_type?: number;
  expire_time?: number;
  multiple?: number;
  positionid?: number;
  ref_id?: number;
  [key: string]: any;
};

export type BTCCOrderListResponse = {
  code: number;
  msg: string;
  orders: BTCCOrderRaw[];
};

export type BTCCTradeReportEntry = {
  dealId: string | number;
  symbol: string;
  orderType: number;
  reportType?: string;
  direction: 'BUY' | 'SELL' | string;
  execPrice?: number | string;
  execVolume?: number | string;
  positionId?: string | number;
  commission?: number | string;
  remark?: string;
  digits?: number | string;
  multiple?: number | string;
  productName?: string;
  currency?: string;
  stoploss?: number | string;
  takeprofit?: number | string;
  profit?: number | string;
  stoptime?: number | string;
  openmargin?: number | string;
  openPrice?: number | string;
  closePrice?: number | string;
  swap?: number | string;
  execTime?: number | string;
  openTime?: number | string;
  tradePrice?: number | string;
  tradeVolume?: number | string;
  openDirection?: 'BUY' | 'SELL' | string;
  tradeTime?: number | string;
};

export type BTCCTradeReportResponse = {
  code: number;
  msg: string;
  list: BTCCTradeReportEntry[];
  pageNo: number;
  pageSize: number;
  totalSize: number;
  totalPage: number;
};

export type BTCCProfitReportEntry = {
  dealId: string | number;
  symbol: string;
  orderType: number;
  openDirection: 'BUY' | 'SELL' | string;
  openPrice: number | string;
  tradeVolume: number | string;
  tradePrice: number | string;
  digits?: number | string;
  swap?: number | string;
  commission?: number | string;
  profit?: number | string;
  openNo?: string | number;
  openTime?: number | string;
  tradeTime?: number | string;
  remark?: string;
  multiple?: number | string;
  productName?: string;
  currency?: string;
};

export type BTCCProfitReportResponse = {
  code: number;
  msg: string;
  list: BTCCProfitReportEntry[];
  pageNo: number;
  pageSize: number;
  totalSize: number;
  totalPage: number;
};

export enum BTCCOrderStatus {
  Initial = 0,
  Pending = 1,
  Filled = 2,
  PartiallyFilled = 4,
  Canceled = 5,
  Rejected = 6,
}

export enum BTCCOrderType {
  LimitOpen = 1,
  StopOpen = 2,
  LimitClose = 3,
  MarketStopLoss = 4,
  MarketTakeProfit = 5,
}

export enum BTCCPositionStatus {
  Initial = 0,
  Open = 1,
  Closed = 2,
}
