pub enum $name { A, B(u32), C { x: u32, y: bool } }

impl minicbor::Encode<()> for $name {
    fn encode<W: minicbor::encode::Write>(&self, e: &mut minicbor::Encoder<W>, _: &mut ()) -> Result<(), minicbor::encode::Error<W::Error>> {
        match self {
            $name::A => { e.str("A")?; }
            $name::B(v) => { e.map(1)?.str("B")?.u32(*v)?; }
            $name::C { x, y } => { e.map(1)?.str("C")?.map(2)?.str("x")?.u32(*x)?.str("y")?.bool(*y)?; }
        }
        Ok(())
    }
}

impl<'b> minicbor::Decode<'b, ()> for $name {
    fn decode(d: &mut minicbor::Decoder<'b>, _: &mut ()) -> Result<Self, minicbor::decode::Error> {
        match d.datatype()? {
            minicbor::data::Type::String => match d.str()? {
                "A" => Ok($name::A),
                _ => Err(minicbor::decode::Error::message("unknown variant")),
            },
            _ => {
                d.map()?;
                match d.str()? {
                    "B" => Ok($name::B(d.u32()?)),
                    "C" => {
                        let mut x = None;
                        let mut y = None;
                        let n = d.map()?.ok_or_else(|| minicbor::decode::Error::message("indefinite map"))?;
                        for _ in 0..n {
                            match d.str()? { "x" => x = Some(d.u32()?), "y" => y = Some(d.bool()?), _ => d.skip()? }
                        }
                        Ok($name::C { x: x.ok_or_else(|| minicbor::decode::Error::missing_value(0))?, y: y.ok_or_else(|| minicbor::decode::Error::missing_value(1))? })
                    }
                    _ => Err(minicbor::decode::Error::message("unknown variant")),
                }
            }
        }
    }
}
