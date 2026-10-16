#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum CallableMethodsInWorker {
    #[serde(rename = "setLeverage")]
    SetLeverage,
    #[serde(rename = "scheduleCancel")]
    ScheduleCancel,
    #[serde(rename = "changePositionMode")]
    ChangePositionMode,
    #[serde(rename = "changeMarginType")]
    ChangeMarginType,
    #[serde(rename = "fetchAndSetPrivateSnapshot")]
    FetchAndSetPrivateSnapshot,
    #[serde(rename = "reconnect")]
    Reconnect,
    #[serde(rename = "fetchOHLCV")]
    FetchOHLCV,
    #[serde(rename = "fetchHistoricalFills")]
    FetchHistoricalFills,
    #[serde(rename = "fetchOrders")]
    FetchOrders,
    #[serde(rename = "fetchAndSetOrders")]
    FetchAndSetOrders,
    #[serde(rename = "updateSymbolMetadata")]
    UpdateSymbolMetadata,
    #[serde(rename = "fetchLastLogsFromWebsockets")]
    FetchLastLogsFromWebsockets,
    #[serde(rename = "removeOrders")]
    RemoveOrders,
    #[serde(rename = "fetchIncomeHistory")]
    FetchIncomeHistory,
    #[serde(rename = "fetchIncomeHistoryRange")]
    FetchIncomeHistoryRange,
    #[serde(rename = "fetchHistoricalOrders")]
    FetchHistoricalOrders,
    #[serde(rename = "fetchClosedPnl")]
    FetchClosedPnl,
}
