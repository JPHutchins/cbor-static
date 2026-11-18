
struct Source<'a>(&'a [u8]);

impl embedded_io::ErrorType for Source<'_> {
    type Error = core::convert::Infallible;
}

impl embedded_io::Read for Source<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let read = buf.iter_mut().zip(self.0).map(|(to, from)| *to = *from).count();
        self.0 = self.0.get(read..).unwrap_or_default();
        Ok(read)
    }
}

struct Sink<'a>(&'a mut [u8]);

impl embedded_io::ErrorType for Sink<'_> {
    type Error = core::convert::Infallible;
}

impl embedded_io::Write for Sink<'_> {
    fn write(&mut self, part: &[u8]) -> Result<usize, Self::Error> {
        let written = self.0.iter_mut().zip(part).map(|(to, from)| *to = *from).count();
        self.0 = core::mem::take(&mut self.0).get_mut(written..).unwrap_or_default();
        Ok(written)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn roundtrip_embedded_io<T: for<'de> cbor_static::Decode<'de> + cbor_static::Encode>(input: &[u8], out: &mut [u8]) -> usize {
    let mut scratch = [0u8; 64];
    let Ok((value, _)) = cbor_static::embedded_io::from_reader::<T, _>(&mut Source(input), &mut scratch) else {
        return 0;
    };
    cbor_static::embedded_io::to_writer(&value, &mut Sink(out), &mut scratch).unwrap_or(0)
}
