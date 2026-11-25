//! Model-based tests (#10, Q37): generated CBOR trees, with random head widths, indefinite
//! containers and chunked strings, then byte mutations. Every walker must agree with the
//! reference on acceptance and length, the streaming skip at every chunk size.

use std::vec::Vec;

use proptest::collection::vec as many;
use proptest::prelude::*;

use super::tree::{encoded, tree};
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
