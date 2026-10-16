fn check_fields(value: &PositionsResponse, expected: &Value) {
    assert_eq!(value.positions.len(),1,"PHEMEX-CARDINALITY baseline native positions");
    let position = value.positions.first().expect("PHEMEX-CARDINALITY fixed native baseline position");
    assert_eq!(
        capture(&value.account.user_id),
        expected["account.userID"],
        "native account.userID"
    );
    assert_eq!(
        capture(&value.account.account_id),
        expected["account.accountId"],
        "native account.accountId"
    );
    assert_eq!(
        capture(&value.account.currency),
        expected["account.currency"],
        "native account.currency"
    );
    assert_eq!(
        capture(&value.account.account_balance_rv),
        expected["account.accountBalanceRv"],
        "native account.accountBalanceRv"
    );
    assert_eq!(
        capture(&value.account.total_used_balance_rv),
        expected["account.totalUsedBalanceRv"],
        "native account.totalUsedBalanceRv"
    );
    assert_eq!(
        capture(&value.account.bonus_balance_rv),
        expected["account.bonusBalanceRv"],
        "native account.bonusBalanceRv"
    );
    assert_eq!(
        capture(&value.account.status),
        expected["account.status"],
        "native account.status"
    );
    assert_eq!(
        capture(&position.user_id),
        expected["positions[0].userID"],
        "native positions[0].userID"
    );
    assert_eq!(
        capture(&position.account_id),
        expected["positions[0].accountID"],
        "native positions[0].accountID"
    );
    assert_eq!(
        capture(&position.symbol),
        expected["positions[0].symbol"],
        "native positions[0].symbol"
    );
    assert_eq!(
        capture(&position.currency),
        expected["positions[0].currency"],
        "native positions[0].currency"
    );
    assert_eq!(
        capture(&position.side),
        expected["positions[0].side"],
        "native positions[0].side"
    );
    assert_eq!(
        capture(&position.position_status),
        expected["positions[0].positionStatus"],
        "native positions[0].positionStatus"
    );
    assert_eq!(
        capture(&position.cross_margin),
        expected["positions[0].crossMargin"],
        "native positions[0].crossMargin"
    );
    assert_eq!(
        capture(&position.leverage_rr),
        expected["positions[0].leverageRr"],
        "native positions[0].leverageRr"
    );
    assert_eq!(
        capture(&position.init_margin_req_rr),
        expected["positions[0].initMarginReqRr"],
        "native positions[0].initMarginReqRr"
    );
    assert_eq!(
        capture(&position.maint_margin_req_rr),
        expected["positions[0].maintMarginReqRr"],
        "native positions[0].maintMarginReqRr"
    );
    assert_eq!(
        capture(&position.risk_limit_rv),
        expected["positions[0].riskLimitRv"],
        "native positions[0].riskLimitRv"
    );
    assert_eq!(
        capture(&position.size),
        expected["positions[0].size"],
        "native positions[0].size"
    );
    assert_eq!(
        capture(&position.value_rv),
        expected["positions[0].valueRv"],
        "native positions[0].valueRv"
    );
    assert_eq!(
        capture(&position.avg_entry_price_rp),
        expected["positions[0].avgEntryPriceRp"],
        "native positions[0].avgEntryPriceRp"
    );
    assert_eq!(
        capture(&position.avg_entry_price),
        expected["positions[0].avgEntryPrice"],
        "native positions[0].avgEntryPrice"
    );
    assert_eq!(
        capture(&position.pos_cost_rv),
        expected["positions[0].posCostRv"],
        "native positions[0].posCostRv"
    );
    assert_eq!(
        capture(&position.assigned_pos_balance_rv),
        expected["positions[0].assignedPosBalanceRv"],
        "native positions[0].assignedPosBalanceRv"
    );
    assert_eq!(
        capture(&position.bankrupt_comm_rv),
        expected["positions[0].bankruptCommRv"],
        "native positions[0].bankruptCommRv"
    );
    assert_eq!(
        capture(&position.bankrupt_price_rp),
        expected["positions[0].bankruptPriceRp"],
        "native positions[0].bankruptPriceRp"
    );
    assert_eq!(
        capture(&position.position_margin_rv),
        expected["positions[0].positionMarginRv"],
        "native positions[0].positionMarginRv"
    );
    assert_eq!(
        capture(&position.liquidation_price_rp),
        expected["positions[0].liquidationPriceRp"],
        "native positions[0].liquidationPriceRp"
    );
    assert_eq!(
        capture(&position.deleverage_percentile_rr),
        expected["positions[0].deleveragePercentileRr"],
        "native positions[0].deleveragePercentileRr"
    );
    assert_eq!(
        capture(&position.buy_value_to_cost_rr),
        expected["positions[0].buyValueToCostRr"],
        "native positions[0].buyValueToCostRr"
    );
    assert_eq!(
        capture(&position.sell_value_to_cost_rr),
        expected["positions[0].sellValueToCostRr"],
        "native positions[0].sellValueToCostRr"
    );
    assert_eq!(
        capture(&position.mark_price_rp),
        expected["positions[0].markPriceRp"],
        "native positions[0].markPriceRp"
    );
    assert_eq!(
        capture(&position.estimated_ord_loss_rv),
        expected["positions[0].estimatedOrdLossRv"],
        "native positions[0].estimatedOrdLossRv"
    );
    assert_eq!(
        capture(&position.used_balance_rv),
        expected["positions[0].usedBalanceRv"],
        "native positions[0].usedBalanceRv"
    );
    assert_eq!(
        capture(&position.cum_closed_pnl_rv),
        expected["positions[0].cumClosedPnlRv"],
        "native positions[0].cumClosedPnlRv"
    );
    assert_eq!(
        capture(&position.cum_funding_fee_rv),
        expected["positions[0].cumFundingFeeRv"],
        "native positions[0].cumFundingFeeRv"
    );
    assert_eq!(
        capture(&position.cum_transact_fee_rv),
        expected["positions[0].cumTransactFeeRv"],
        "native positions[0].cumTransactFeeRv"
    );
    assert_eq!(
        capture(&position.transact_time_ns),
        expected["positions[0].transactTimeNs"],
        "native positions[0].transactTimeNs"
    );
    assert_eq!(
        capture(&position.taker_fee_rate_rr),
        expected["positions[0].takerFeeRateRr"],
        "native positions[0].takerFeeRateRr"
    );
    assert_eq!(
        capture(&position.maker_fee_rate_rr),
        expected["positions[0].makerFeeRateRr"],
        "native positions[0].makerFeeRateRr"
    );
    assert_eq!(
        capture(&position.term),
        expected["positions[0].term"],
        "native positions[0].term"
    );
    assert_eq!(
        capture(&position.last_term_end_time_ns),
        expected["positions[0].lastTermEndTimeNs"],
        "native positions[0].lastTermEndTimeNs"
    );
    assert_eq!(
        capture(&position.last_funding_time_ns),
        expected["positions[0].lastFundingTimeNs"],
        "native positions[0].lastFundingTimeNs"
    );
    assert_eq!(
        capture(&position.cur_term_realised_pnl_rv),
        expected["positions[0].curTermRealisedPnlRv"],
        "native positions[0].curTermRealisedPnlRv"
    );
    assert_eq!(
        capture(&position.exec_seq),
        expected["positions[0].execSeq"],
        "native positions[0].execSeq"
    );
    assert_eq!(
        capture(&position.pos_side),
        expected["positions[0].posSide"],
        "native positions[0].posSide"
    );
    assert_eq!(
        capture(&position.pos_mode),
        expected["positions[0].posMode"],
        "native positions[0].posMode"
    );
    assert_eq!(
        capture(&position.buy_leaves_value_rv),
        expected["positions[0].buyLeavesValueRv"],
        "native positions[0].buyLeavesValueRv"
    );
    assert_eq!(
        capture(&position.sell_leaves_value_rv),
        expected["positions[0].sellLeavesValueRv"],
        "native positions[0].sellLeavesValueRv"
    );
}
