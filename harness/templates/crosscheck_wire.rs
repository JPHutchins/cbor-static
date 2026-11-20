use crosscheck::{flat, rich};

fn hand<T: minicbor::Encode<()>>(value: &T) -> Vec<u8> {
    let mut buf = [0u8; 1024];
    let mut cursor = minicbor::encode::write::Cursor::new(&mut buf[..]);
    minicbor::encode(value, &mut cursor).expect("minicbor encodes");
    let len = cursor.position();
    buf[..len].to_vec()
}

fn stat<T: cbor_static::Encode>(value: &T) -> Vec<u8> {
    let mut buf = [0u8; 1024];
    let len = cbor_static::to_slice(value, &mut buf).expect("cbor-static encodes");
    buf[..len].to_vec()
}

fn serde_layer<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut buf = [0u8; 1024];
    let len = cbor_static::serde::to_slice(value, &mut buf).expect("the serde layer encodes");
    buf[..len].to_vec()
}

/// R-302: one definition, both layers: the same bytes, and each decodes the other's.
macro_rules! layers {
    ($$ty:ty, $$value:expr) => {{
        let value = $$value;
        let native = stat(&value);
        let through_serde = serde_layer(&value);
        assert_eq!(through_serde, native);
        let decoded: $$ty = cbor_static::serde::from_slice(&native).expect("the serde layer decodes native bytes");
        assert_eq!(stat(&decoded), native);
        let mut slot = core::mem::MaybeUninit::<$$ty>::uninit();
        let decoded = cbor_static::decode_into(&through_serde, &mut slot).expect("cbor-static decodes the serde layer's bytes");
        assert_eq!(serde_layer(&*decoded), native);
    }};
}

macro_rules! agree {
    ($$hand_ty:ty, $$stat_ty:ty, $$hand:expr, $$stat:expr) => {{
        let from_hand = hand(&$$hand);
        let from_stat = stat(&$$stat);
        assert_eq!(from_stat, from_hand);
        let decoded: $$hand_ty = minicbor::decode(&from_stat).expect("minicbor decodes cbor-static bytes");
        assert_eq!(hand(&decoded), from_stat);
        let mut slot = core::mem::MaybeUninit::<$$stat_ty>::uninit();
        let decoded = cbor_static::decode_into(&from_hand, &mut slot).expect("cbor-static decodes minicbor bytes");
        assert_eq!(stat(&*decoded), from_hand);
    }};
}

$tests
