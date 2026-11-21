//! RFC 9528 Appendix C.2's CDDL with erratum 8258's fix, and RFC 9052, RFC 9053 and RFC 9711
//! Appendix D's for the COSE structures and claims sets it carries, all extracted from the RFCs'
//! XML, against what the EDHOC types write: one case per trace item. The `cddl` crate checks an
//! item against the rule named as the document's root; a group, sent as a CBOR sequence, is
//! checked as the array of its items.

use crate::items::{Item, reencoded};

const RFC_9528_ERRATUM_8258: &str =
    include_str!(concat!(env!("RFC_CDDL"), "/rfc9528-erratum-8258.cddl"));
const RFC_9052: &str = include_str!(concat!(env!("RFC_CDDL"), "/rfc9052.cddl"));
const RFC_9053: &str = include_str!(concat!(env!("RFC_CDDL"), "/rfc9053.cddl"));
const RFC_9711_APPENDIX_D: &str =
    include_str!(concat!(env!("RFC_CDDL"), "/rfc9711-appendix-d.cddl"));

/// The CDDL crate's verdict on `item` against `rule`.
fn validated(rule: &str, item: &[u8]) -> Result<(), cddl::validator::cbor::Error<std::io::Error>> {
    cddl::validate_cbor_from_slice(
        &format!(
            "cbor-static-root = {rule}\n{RFC_9528_ERRATUM_8258}\n{RFC_9052}\n{RFC_9053}\n{RFC_9711_APPENDIX_D}"
        ),
        item,
        Some(&["cbor"]),
    )
}

fn matches(rule: &str, item: &[u8]) -> Result<(), String> {
    validated(rule, item).map_err(|error| error.to_string())
}

/// The sequence's items as one array; every byte must belong to a whole item.
fn as_array(sequence: &[u8]) -> Vec<u8> {
    let count = core::iter::successors(Some(sequence), |rest| {
        (!rest.is_empty()).then(|| {
            cbor_static::from_slice_partial::<cbor_static::RawCbor>(rest)
                .map(|(_, after)| after)
                .expect("a whole item")
        })
    })
    .count()
        - 1;
    [
        vec![
            0x80 | u8::try_from(count)
                .ok()
                .filter(|count| *count < 24)
                .expect("fewer than 24"),
        ],
        sequence.to_vec(),
    ]
    .concat()
}

/// How an item is checked: as one item against a rule, as a sequence against a group, or not at
/// all where Appendix C.2 names no rule for it.
#[derive(Debug, PartialEq, Eq)]
enum Check {
    Item(&'static str),
    Group(&'static str),
    Unnamed,
}

fn check(item: Item, written: &[u8]) -> Check {
    match item {
        Item::Message1 => Check::Group("[message_1]"),
        Item::Message2 => Check::Group("[message_2]"),
        Item::Message3 => Check::Group("[message_3]"),
        Item::Message4 => Check::Group("[message_4]"),
        Item::Plaintext2 => Check::Group("[PLAINTEXT_2]"),
        Item::Plaintext3 => Check::Group("[PLAINTEXT_3]"),
        Item::Plaintext4 => Check::Group("[PLAINTEXT_4]"),
        Item::Error => Check::Group("[error]"),
        Item::Ead1 => Check::Group("[? EAD_1]"),
        Item::Ead2 => Check::Group("[? EAD_2]"),
        Item::Ead3 => Check::Group("[? EAD_3]"),
        Item::Ead4 => Check::Group("[? EAD_4]"),
        Item::InfoForIv3
        | Item::InfoForIv4
        | Item::InfoForK3
        | Item::InfoForK4
        | Item::InfoForKeyUpdate
        | Item::InfoForKeystream2
        | Item::InfoForMac2
        | Item::InfoForMac3
        | Item::InfoForOscoreMasterSalt
        | Item::InfoForOscoreMasterSecret
        | Item::InfoForPrkExporter
        | Item::InfoForPrkOut
        | Item::InfoForSalt3e2m
        | Item::InfoForSalt4e3m => Check::Group("[info]"),
        Item::SuitesI => Check::Item("suites"),
        Item::IdCredI | Item::IdCredR => Check::Item("header_map"),
        Item::CredI | Item::CredR if written.first().is_some_and(|head| head >> 5 == 5) => {
            Check::Item("Claims-Set")
        }
        Item::MessageToBeSignedInMessage2 | Item::MessageToBeSignedInMessage3 => {
            Check::Item("Sig_structure")
        }
        Item::A3 | Item::A4 => Check::Item("Enc_structure"),
        Item::CredI
        | Item::CredR
        | Item::Method
        | Item::GX
        | Item::GY
        | Item::CI
        | Item::CR
        | Item::HMessage1
        | Item::Th2
        | Item::Th3
        | Item::Th4
        | Item::Mac2
        | Item::Mac3
        | Item::SignatureOrMac2
        | Item::SignatureOrMac3
        | Item::ContextForKeyUpdate
        | Item::Context2
        | Item::Context3
        | Item::InputToCalculateTh2
        | Item::InputToCalculateTh3
        | Item::InputToCalculateTh4 => Check::Unnamed,
    }
}

/// The item as the types re-encode it matches its rule.
fn item_matches(item: Item, input: &[u8]) {
    let written = reencoded(item, input).expect("re-encodes");
    match check(item, &written) {
        Check::Item(rule) => assert_eq!(matches(rule, &written), Ok(()), "{rule}"),
        Check::Group(rule) => assert_eq!(matches(rule, &as_array(&written)), Ok(()), "{rule}"),
        Check::Unnamed => {}
    }
}

macro_rules! rfc9529_cases {
    ($(($test:ident, $section:literal, $item:ident)),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                item_matches(Item::$item, rfc9529!($test));
            }
        )*

        const ITEMS: &[(Item, &[u8])] = &[$((Item::$item, rfc9529!($test))),*];
    };
}

include!(concat!(env!("RFC_VECTORS"), "/rfc9529_cases.rs"));

#[test]
fn every_item_c2_names_is_checked() {
    let checks = ITEMS
        .iter()
        .map(|(item, input)| check(*item, &reencoded(*item, input).expect("re-encodes")))
        .collect::<Vec<_>>();
    assert_eq!(
        (
            checks
                .iter()
                .filter(|c| matches!(c, Check::Group(_)))
                .count(),
            checks
                .iter()
                .filter(|c| matches!(c, Check::Item(_)))
                .count(),
            checks.iter().filter(|c| **c == Check::Unnamed).count(),
        ),
        (51, 15, 43)
    );
}

mod invalid {
    use super::{Check, Item, as_array, check, validated};

    /// The invalid items Appendix C.2 itself rejects: an array for a sequence, a suites array of
    /// one, a text `G_X`, and a second item in `message_2`. The rest break rules its CDDL does not
    /// state, which `crate::invalid` classifies.
    const REJECTED_BY_THE_CDDL: [&str; 4] = ["4.1.1", "4.1.3", "4.1.4", "4.1.5"];

    fn verdict(section: &str, item: Item, input: &[u8]) {
        let Check::Group(rule) = check(item, input) else {
            panic!("{item:?} is not a group")
        };
        match validated(rule, &as_array(input)) {
            Ok(()) => assert!(!REJECTED_BY_THE_CDDL.contains(&section)),
            Err(cddl::validator::cbor::Error::Validation(_)) => {
                assert!(REJECTED_BY_THE_CDDL.contains(&section));
            }
            Err(other) => panic!("not a validation verdict: {other}"),
        }
    }

    macro_rules! rfc9529_invalid_cases {
        ($(($test:ident, $section:literal, $item:ident)),* $(,)?) => {
            $(
                #[test]
                fn $test() {
                    verdict($section, Item::$item, rfc9529!($test));
                }
            )*
        };
    }

    include!(concat!(env!("RFC_VECTORS"), "/rfc9529_invalid_cases.rs"));
}
