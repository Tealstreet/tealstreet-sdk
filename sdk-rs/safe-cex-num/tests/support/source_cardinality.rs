pub fn for_each<T>(
    rows: impl IntoIterator<Item = T>,
    total: usize,
    name: &str,
    mut check: impl FnMut(T),
) {
    let mut rows: Vec<_> = rows.into_iter().collect();
    let control = std::env::var("SOURCE_CARDINALITY_CONTROL").unwrap_or_default();
    match control.strip_prefix(&format!("{name}:")) {
        Some("empty") => rows.clear(),
        Some("subset") => {
            rows.pop();
        }
        Some("after") | None => {}
        Some(other) => panic!("unknown source cardinality control {other}"),
    }
    assert_eq!(rows.len(), total, "SOURCE-CARDINALITY {name} source total");
    if control == format!("{name}:after") {
        rows.clear();
    }
    let mut compared = 0;
    for row in rows {
        check(row);
        compared += 1;
    }
    assert_eq!(compared, total, "SOURCE-CARDINALITY {name} compared total");
}
