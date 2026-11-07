use super::*;

fn record(id: u64, data: usize) -> Record {
    Record::new(
        Owner::Command {
            id: 7,
            generation: 1,
        },
        id,
        Attempt {
            request: RequestId(id),
            generation: 1,
        },
        20_000.,
        ReconciliationData::new("x".repeat(data)).unwrap(),
    )
}

#[test]
fn a_group_inserts_every_member_and_preserves_prior_records() {
    let mut table = Responses::default();
    table.insert(record(9, 3), 1.).unwrap();
    table.start(RequestId(9), 200, "prior-header").unwrap();
    table.chunk(RequestId(9), b"prior-body").unwrap();
    let before = table.bytes;
    let records = vec![record(1, 5), record(2, 7)];
    let added: usize = records.iter().map(Record::bytes).sum();
    table.insert_group(records, 2.).unwrap();
    assert_eq!(table.bytes, before + added);
    assert_eq!(table.slots.len(), 3);
    assert!(table.contains(RequestId(1)) && table.contains(RequestId(2)));
    let prior = table.get_mut(RequestId(9)).unwrap();
    assert_eq!(prior.headers, "prior-header");
    assert_eq!(prior.body, b"prior-body");
}

#[test]
fn duplicate_members_leave_no_partial_group_or_changed_accounting() {
    let mut table = Responses::default();
    table.insert(record(9, 3), 1.).unwrap();
    let before = table.bytes;
    for duplicate in [1, 9] {
        assert_eq!(
            table.insert_group(vec![record(1, 0), record(duplicate, 0)], 2.),
            Err(Error::Duplicate)
        );
        assert_eq!(table.bytes, before);
        assert_eq!(table.slots.len(), 1);
        assert!(!table.contains(RequestId(1)));
        assert_eq!(table.get_mut(RequestId(9)).unwrap().data.as_str(), "xxx");
    }
}

#[test]
fn an_expired_member_cannot_prune_or_publish_any_record() {
    let mut table = Responses::default();
    table.insert(record(9, 3), 1.).unwrap();
    let before = table.bytes;
    let mut expired = record(2, 0);
    expired.expires_at = 2.;
    assert_eq!(
        table.insert_group(vec![record(1, 0), expired], 2.),
        Err(Error::TimedOut)
    );
    assert_eq!(table.bytes, before);
    assert_eq!(table.slots.len(), 1);
    assert!(table.contains(RequestId(9)));
    assert!(!table.contains(RequestId(1)) && !table.contains(RequestId(2)));
}

#[test]
fn delivering_slots_and_aggregate_bytes_remain_reserved_on_group_refusal() {
    let mut table = Responses::default();
    for id in 1..CAPACITY as u64 {
        table.insert(record(id, 0), 1.).unwrap();
    }
    table.start(RequestId(1), 200, "").unwrap();
    table.chunk(RequestId(1), b"{}").unwrap();
    let delivering = table.complete(RequestId(1)).unwrap();
    let before = table.bytes;
    assert_eq!(
        table.insert_group(vec![record(300, 0), record(301, 0)], 2.),
        Err(Error::Capacity)
    );
    assert_eq!(table.bytes, before);
    assert_eq!(table.slots.len(), CAPACITY - 1);
    assert!(matches!(
        table.slots.get(&RequestId(1)),
        Some(Slot::Delivering(_))
    ));
    assert!(!table.contains(RequestId(300)) && !table.contains(RequestId(301)));
    assert_eq!(delivering.body, b"{}");

    let mut table = Responses::default();
    for id in 1..16 {
        table.insert(record(id, 0), 1.).unwrap();
        table.start(RequestId(id), 200, "").unwrap();
        table.chunk(RequestId(id), &vec![b'x'; BODY_LIMIT]).unwrap();
    }
    let mut id = 100;
    while TOTAL_LIMIT - table.bytes >= record(id, DATA_LIMIT).bytes() {
        table.insert(record(id, DATA_LIMIT), 1.).unwrap();
        id += 1;
    }
    let before = table.bytes;
    assert_eq!(
        table.insert_group(vec![record(id, 0), record(id + 1, DATA_LIMIT)], 2.),
        Err(Error::Capacity)
    );
    assert_eq!(table.bytes, before);
    assert!(!table.contains(RequestId(id)) && !table.contains(RequestId(id + 1)));
    assert_eq!(table.get_mut(RequestId(1)).unwrap().body.len(), BODY_LIMIT);
}
