impl$impl_lifetime minicbor::Encode<()> for $name$lifetime {
    fn encode<W: minicbor::encode::Write>(&self, e: &mut minicbor::Encoder<W>, _: &mut ()) -> Result<(), minicbor::encode::Error<W::Error>> {
        e.map($entries)?;
$encode_fields        Ok(())
    }
}

impl<$decode_lifetime> minicbor::Decode<$decode_lifetime, ()> for $name$lifetime {
    fn decode(d: &mut minicbor::Decoder<$decode_lifetime>, _: &mut ()) -> Result<Self, minicbor::decode::Error> {
$lets        let entries = d.map()?.ok_or_else(|| minicbor::decode::Error::message("indefinite map"))?;
        for _ in 0..entries {
            match d.str()? {
$arms                _ => d.skip()?,
            }
        }
        Ok($name {
$finish        })
    }
}
