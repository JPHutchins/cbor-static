//! Generated trees, from the generator cbor-static's well-formedness model uses, read alike by
//! every oracle. The rows the cases reach are counted: printed, and written as JSON where
//! `DAILY_TALLY` names a file, for the daily report.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::common::differential::legs::Row;

#[derive(Serialize)]
struct Tally {
    rows: BTreeMap<Row, usize>,
}

#[test]
fn generated_trees_read_alike_in_every_oracle() {
    use std::cell::RefCell;

    use proptest::test_runner::{Config, TestRunner};

    use crate::common::differential::legs;
    use crate::tree::{encoded, tree};

    let tally = RefCell::new(Tally {
        rows: Row::ALL.iter().map(|&row| (row, 0)).collect(),
    });
    let config = Config {
        failure_persistence: None,
        ..Config::default()
    };
    TestRunner::new(config.clone())
        .run(&tree(), |tree| {
            for row in legs::agree(&encoded(&tree)) {
                *tally.borrow_mut().rows.entry(row).or_default() += 1;
            }
            Ok(())
        })
        .unwrap_or_else(|failure| panic!("{failure}"));
    println!(
        "{cases} generated items; normalization rows reached: {rows:?}",
        cases = config.cases,
        rows = tally.borrow().rows
    );
    if let Some(path) = std::env::var_os("DAILY_TALLY") {
        std::fs::write(
            path,
            serde_json::to_string(&*tally.borrow()).expect("a tally serializes"),
        )
        .expect("the tally is written");
    }
}
