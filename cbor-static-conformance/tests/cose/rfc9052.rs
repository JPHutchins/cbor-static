//! RFC 9052 Appendix B and C's message examples. The RFC prints diagnostic notation and each
//! binary's size; the binaries are cose-wg/Examples' `RFC8152/` files (RFC 9052 Appendix C points
//! there), matched to sections by title and size.

use crate::common::vectors::{COSE_WG, Kind, vector};
use crate::decoded::{kind, message};

/// The example is the size its section states, and decodes as the message its diagnostic notation
/// opens with.
fn stated_size_and_type(name: &str, size: usize, expected: Kind) {
    let vector = vector(name);
    assert_eq!(
        (
            vector.output.len(),
            message(vector).map(|decoded| kind(&decoded))
        ),
        (size, Ok(expected))
    );
}

/// One test per RFC 9052 section, named after it, with its cose-wg file, stated size and message
/// type; and the files, for the check of which ones RFC 9052 dropped.
macro_rules! examples {
    ($(($section:ident, $name:literal, $size:literal, $kind:ident)),* $(,)?) => {
        $(
            #[test]
            fn $section() {
                stated_size_and_type($name, $size, Kind::$kind);
            }
        )*

        const FILES: &[&str] = &[$($name),*];
    };
}

examples! {
    (appendix_b, "RFC8152/Appendix_B", 183, Encrypt),
    (appendix_c_1_1, "RFC8152/Appendix_C_1_1", 103, Sign),
    (appendix_c_1_2, "RFC8152/Appendix_C_1_2", 277, Sign),
    (appendix_c_1_3, "RFC8152/Appendix_C_1_4", 125, Sign),
    (appendix_c_2_1, "RFC8152/Appendix_C_2_1", 98, Sign1),
    (appendix_c_3_1, "RFC8152/Appendix_C_3_1", 151, Encrypt),
    (appendix_c_3_2, "RFC8152/Appendix_C_3_2", 91, Encrypt),
    (appendix_c_3_3, "RFC8152/Appendix_C_3_4", 173, Encrypt),
    (appendix_c_4_1, "RFC8152/Appendix_C_4_1", 52, Encrypt0),
    (appendix_c_4_2, "RFC8152/Appendix_C_4_2", 41, Encrypt0),
    (appendix_c_5_1, "RFC8152/Appendix_C_5_1", 57, Mac),
    (appendix_c_5_2, "RFC8152/Appendix_C_5_2", 214, Mac),
    (appendix_c_5_3, "RFC8152/Appendix_C_5_3", 109, Mac),
    (appendix_c_5_4, "RFC8152/Appendix_C_5_4", 309, Mac),
    (appendix_c_6_1, "RFC8152/Appendix_C_6_1", 37, Mac0),
}

#[test]
fn the_rfc_8152_files_rfc_9052_dropped_are_its_countersignature_examples() {
    assert_eq!(
        COSE_WG
            .iter()
            .filter(|vector| vector.name.starts_with("RFC8152/"))
            .filter(|vector| !FILES.contains(&vector.name))
            .map(|vector| vector.name)
            .collect::<Vec<_>>(),
        ["RFC8152/Appendix_C_1_3", "RFC8152/Appendix_C_3_3"]
    );
}
