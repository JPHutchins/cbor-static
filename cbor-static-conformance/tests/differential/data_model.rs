//! Generated trees, from the generator cbor-static's well-formedness model uses, read alike by
//! every oracle. The rows each case reached are counted and printed.

use std::cell::RefCell;
use std::collections::BTreeMap;

use proptest::test_runner::{Config, TestRunner};

use crate::common::differential::legs::{self, Row};
use crate::tree::{encoded, tree};

#[test]
fn generated_trees_read_alike_in_every_oracle() {
    let rows = RefCell::new(BTreeMap::<Row, usize>::new());
    let config = Config {
        failure_persistence: None,
        ..Config::default()
    };
    TestRunner::new(config.clone())
        .run(&tree(), |tree| {
            for row in legs::agree(&encoded(&tree)) {
                *rows.borrow_mut().entry(row).or_default() += 1;
            }
            Ok(())
        })
        .unwrap_or_else(|failure| panic!("{failure}"));
    println!(
        "{cases} generated items; normalization rows reached: {rows:?}",
        cases = config.cases,
        rows = rows.borrow()
    );
}
