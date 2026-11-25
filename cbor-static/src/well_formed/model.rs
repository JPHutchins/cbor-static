//! Model-based tests (#10, Q37): generated CBOR trees, with random head widths, indefinite
//! containers and chunked strings, then byte mutations. Every walker must agree with the
//! reference on acceptance and length, the streaming skip at every chunk size.

use std::vec::Vec;

use proptest::collection::vec as many;
use proptest::prelude::*;

use super::tree::{Tree, encoded, tree};
use super::{agree, chunk_sizes, oracle};

#[derive(Clone, Debug)]
enum Mutation {
    InsertBreak(usize),
    Insert(usize, u8),
    Delete(usize),
    Flip(usize, u8),
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 4 } else { 512 },
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        any::<usize>().prop_map(Mutation::InsertBreak),
        (any::<usize>(), any::<u8>()).prop_map(|(at, byte)| Mutation::Insert(at, byte)),
        any::<usize>().prop_map(Mutation::Delete),
        (any::<usize>(), 0u8..8).prop_map(|(at, bit)| Mutation::Flip(at, bit)),
    ]
}

fn mutated(mut bytes: Vec<u8>, mutations: &[Mutation]) -> Vec<u8> {
    for mutation in mutations {
        let len = bytes.len();
        match *mutation {
            Mutation::InsertBreak(at) => bytes.insert(at % (len + 1), 0xff),
            Mutation::Insert(at, byte) => bytes.insert(at % (len + 1), byte),
            Mutation::Delete(at) if len > 0 => {
                bytes.remove(at % len);
            }
            Mutation::Flip(at, bit) if len > 0 => bytes[at % len] ^= 1 << bit,
            Mutation::Delete(_) | Mutation::Flip(..) => {}
        }
    }
    bytes
}

/// Whether `tree` holds a tag 2 or 3 over at least `len` bytes.
fn has_bignum_of(len: usize, tree: &Tree) -> bool {
    match tree {
        Tree::Tag {
            number: 2 | 3,
            content,
            ..
        } if matches!(&**content, Tree::Bytes { payload, .. } if payload.len() >= len) => true,
        Tree::Tag { content, .. } => has_bignum_of(len, content),
        Tree::Array { items, .. } => items.iter().any(|item| has_bignum_of(len, item)),
        Tree::Map { entries, .. } => entries
            .iter()
            .any(|(key, value)| has_bignum_of(len, key) || has_bignum_of(len, value)),
        Tree::Int { .. }
        | Tree::Bytes { .. }
        | Tree::Text { .. }
        | Tree::Chunked { .. }
        | Tree::Simple(_)
        | Tree::Float { .. } => false,
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn the_generator_reaches_bignums_of_16_bytes_and_more() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let trees = tree();
    assert!((0..10_000).any(|_| {
        has_bignum_of(
            16,
            &trees
                .new_tree(&mut runner)
                .expect("a tree generates")
                .current(),
        )
    }));
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn generated_trees_are_well_formed_in_every_walker(tree in tree()) {
        let bytes = encoded(&tree);
        prop_assert_eq!(oracle::item(&bytes), Ok(bytes.len()));
        agree(&bytes, chunk_sizes(bytes.len()));
    }

    #[test]
    fn mutated_trees_agree_with_the_reference(tree in tree(), mutations in many(mutation(), 1..4)) {
        let bytes = mutated(encoded(&tree), &mutations);
        agree(&bytes, chunk_sizes(bytes.len()));
    }
}
