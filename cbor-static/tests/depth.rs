#![forbid(unsafe_code)]
#![recursion_limit = "1024"]

mod common;

type By2<T> = [[T; 1]; 1];
type By4<T> = By2<By2<T>>;
type By8<T> = By4<By4<T>>;
type By16<T> = By8<By8<T>>;
type By32<T> = By16<By16<T>>;
type By64<T> = By32<By32<T>>;
type By128<T> = By64<By64<T>>;
type Levels255 = By128<By64<By32<By16<By8<By4<By2<[u8; 1]>>>>>>>;

#[test]
fn the_deepest_type_round_trips() {
    use common::golden;

    assert_eq!(<Levels255 as cbor_static::Encode>::MAX_DEPTH, 255);
    golden(
        &Levels255::default(),
        &[[0x81; 255].as_slice(), &[0x00]].concat(),
    );
}

#[cfg(feature = "heapless")]
mod struct_keys {
    use cbor_static::Cbor;
    use heapless::LinearMap;

    #[derive(Cbor, Debug, PartialEq, Eq)]
    struct K0 {
        a: u8,
    }

    macro_rules! key {
        ($name:ident, $inner:ty) => {
            #[derive(Cbor, Debug, PartialEq, Eq)]
            struct $name {
                m: LinearMap<$inner, u8, 1>,
            }
        };
    }

    key!(K1, K0);
    key!(K2, K1);
    key!(K3, K2);
    key!(K4, K3);
    key!(K5, K4);

    fn holding<K: Eq>(key: K) -> LinearMap<K, u8, 1> {
        LinearMap::from_iter([(key, 7)])
    }

    /// `{"m": {key: 7}}` around `{"a": 1}`, `levels` times.
    fn key_bytes(levels: usize) -> Vec<u8> {
        (0..levels).fold(b"\xa1\x61a\x01".to_vec(), |inner, _| {
            [b"\xa1\x61m\xa1".as_slice(), &inner, b"\x07"].concat()
        })
    }

    #[test]
    fn a_map_keyed_five_struct_keyed_maps_deep_round_trips() {
        assert_eq!(
            <LinearMap<K5, u8, 1> as cbor_static::Encode>::MAX_DEPTH,
            189
        );
        super::common::golden_in_place(
            &holding(K5 {
                m: holding(K4 {
                    m: holding(K3 {
                        m: holding(K2 {
                            m: holding(K1 {
                                m: holding(K0 { a: 1 }),
                            }),
                        }),
                    }),
                }),
            }),
            &[b"\xa1".as_slice(), &key_bytes(5), b"\x07"].concat(),
        );
    }
}
