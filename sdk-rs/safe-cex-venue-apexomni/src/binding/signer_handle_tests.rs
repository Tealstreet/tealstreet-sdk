use super::*;

#[test]
fn native_contract_handle_preserves_payload_identity_and_releases_after_last_alias() {
    for (account, direction, fees) in [(7.0, true, [3.0, 5.0]), (17.0, false, [11.0, 13.0])] {
        let signer = NativeSigner::module();
        let builder: ZkLinkHandle = signer
            .builder(
                TransactionKind::Contract,
                vec![
                    J::Number(account),
                    J::Number(2.0),
                    J::Number(23.0),
                    J::Number(29.0),
                    J::Number(31.0),
                    J::String("37.25".into()),
                    J::String("41.5".into()),
                    J::Bool(direction),
                    J::Number(fees[0]),
                    J::Number(fees[1]),
                    J::Bool(false),
                ],
            )
            .unwrap();
        let weak = Rc::downgrade(&builder);
        let transaction = signer
            .new_transaction(TransactionKind::Contract, builder.clone())
            .unwrap();
        assert!(Rc::ptr_eq(&builder, &transaction));
        let contract = transaction.downcast_ref::<Contract>().unwrap();
        assert_eq!(contract.account_id, account as u32);
        assert_eq!(contract.sub_account_id, 2);
        assert_eq!(contract.slot_id, 23);
        assert_eq!(contract.nonce, 29);
        assert_eq!(contract.pair_id, 31);
        assert_eq!(contract.size, "37.25");
        assert_eq!(contract.price, "41.5");
        assert_eq!(contract.direction, direction);
        assert_eq!(contract.fee_rates, [fees[0] as u8, fees[1] as u8]);
        assert!(!contract.has_subsidy);
        assert_eq!(
            signer
                .create_signed_contract(transaction.clone())
                .unwrap_err()
                .message,
            "zkLink signer is not initialized"
        );
        drop(builder);
        assert!(weak.upgrade().is_some());
        drop(transaction);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn native_contract_handle_refuses_foreign_opaque_payloads() {
    let signer = NativeSigner::module();
    let handle: ZkLinkHandle = Rc::new(std::cell::Cell::new(19));
    let same = handle.clone();
    let error = match signer.new_transaction(TransactionKind::Contract, handle) {
        Ok(_) => panic!("foreign opaque handle must refuse"),
        Err(error) => error,
    };
    assert_eq!(
        error.message,
        "ApexOmni native zkLink capability supports contracts only"
    );
    assert_eq!(
        same.downcast_ref::<std::cell::Cell<i32>>().unwrap().get(),
        19
    );
    assert_eq!(
        signer.create_signed_contract(same).unwrap_err().message,
        "ApexOmni native zkLink capability supports contracts only"
    );
}
