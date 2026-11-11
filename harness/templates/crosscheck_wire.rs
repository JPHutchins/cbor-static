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
    let len = cbor_static::encode_into(value, &mut buf).expect("cbor-static encodes");
    buf[..len].to_vec()
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
