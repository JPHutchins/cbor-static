#![forbid(unsafe_code)]
#![cfg(all(feature = "embedded-io", feature = "std"))]
//! R-206 (#5 Q6, Q16): `to_writer` and `from_reader` over `embedded-io` and `std::io` are thin loops
//! over the resumable engine. Readers and writers that move any number of bytes per call give the
//! same bytes and values, their errors come back as `Io`, and a stream that ends early is
//! `UnexpectedEnd` at the bytes read.

use cbor_static::{Cbor, ErrorKind, IoError, Tagged};

#[derive(Cbor, Debug, PartialEq)]
struct Reading {
    id: u32,
    name: String,
    samples: Vec<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<Tagged<1, u64>>,
    unit: char,
}

fn reading() -> (Reading, Vec<u8>) {
    let value = Reading {
        id: 70_000,
        name: "thermistor".into(),
        samples: vec![-1, 300, i16::MIN, 7],
        at: Some(Tagged(1_700_000_000)),
        unit: 'é',
    };
    let mut out = vec![0u8; 128];
    let len = cbor_static::to_slice(&value, &mut out).expect("encodes");
    out.truncate(len);
    (value, out)
}

/// A call to a mock reader or writer: where it fails, accepts nothing, or moves up to `step` bytes.
#[derive(Clone, Copy)]
struct Script {
    step: usize,
    fail_at: Option<usize>,
    zero_at: Option<usize>,
}

impl Script {
    const fn steps(step: usize) -> Self {
        Self {
            step,
            fail_at: None,
            zero_at: None,
        }
    }
}

#[derive(Debug, PartialEq)]
struct Failed(usize);

impl core::fmt::Display for Failed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "failed at call {call}", call = self.0)
    }
}

impl core::error::Error for Failed {}

impl embedded_io::Error for Failed {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

struct Source<'a> {
    bytes: &'a [u8],
    script: Script,
    calls: usize,
}

impl Source<'_> {
    fn next(&mut self, buf: &mut [u8]) -> Result<usize, Failed> {
        let call = self.calls;
        self.calls += 1;
        if self.script.fail_at == Some(call) {
            return Err(Failed(call));
        }
        let n = self.script.step.min(buf.len()).min(self.bytes.len());
        buf[..n].copy_from_slice(&self.bytes[..n]);
        self.bytes = &self.bytes[n..];
        Ok(n)
    }
}

struct Sink {
    out: Vec<u8>,
    script: Script,
    calls: usize,
    flushes: usize,
}

impl Sink {
    fn next(&mut self, bytes: &[u8]) -> Result<usize, Failed> {
        let call = self.calls;
        self.calls += 1;
        if self.script.fail_at == Some(call) {
            return Err(Failed(call));
        }
        let n = if self.script.zero_at == Some(call) {
            0
        } else {
            self.script.step.min(bytes.len())
        };
        self.out.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
}

impl embedded_io::ErrorType for Source<'_> {
    type Error = Failed;
}

impl embedded_io::Read for Source<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Failed> {
        self.next(buf)
    }
}

impl embedded_io::ErrorType for Sink {
    type Error = Failed;
}

impl embedded_io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> Result<usize, Failed> {
        self.next(bytes)
    }

    fn flush(&mut self) -> Result<(), Failed> {
        self.flushes += 1;
        Ok(())
    }
}

impl std::io::Read for Source<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.next(buf).map_err(std::io::Error::other)
    }
}

impl std::io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.next(bytes).map_err(std::io::Error::other)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

/// An `std::io` error as the `Failed` it wraps, or its kind.
fn failed(error: IoError<std::io::Error>) -> IoError<Result<Failed, std::io::ErrorKind>> {
    match error {
        IoError::Io(error) => IoError::Io(
            error
                .into_inner()
                .and_then(|inner| inner.downcast::<Failed>().ok())
                .map(|failed| *failed)
                .ok_or(std::io::ErrorKind::Other),
        ),
        IoError::Cbor(error) => IoError::Cbor(error),
    }
}

fn source(bytes: &[u8], script: Script) -> Source<'_> {
    Source {
        bytes,
        script,
        calls: 0,
    }
}

fn sink(script: Script) -> Sink {
    Sink {
        out: Vec::new(),
        script,
        calls: 0,
        flushes: 0,
    }
}

fn read_embedded(
    bytes: &[u8],
    script: Script,
    buf: usize,
) -> (Result<Reading, IoError<Failed>>, usize) {
    let mut reader = source(bytes, script);
    let read = cbor_static::embedded_io::from_reader::<Reading, _>(&mut reader, &mut vec![0; buf])
        .map(|(value, rest)| {
            assert!(rest.is_empty(), "nothing follows the item");
            value
        });
    (read, reader.calls)
}

fn read_std(bytes: &[u8], script: Script, buf: usize) -> (Result<Reading, IoError<Failed>>, usize) {
    let mut reader = source(bytes, script);
    let read = cbor_static::io::from_reader::<Reading, _>(&mut reader, &mut vec![0; buf])
        .map(|(value, rest)| {
            assert!(rest.is_empty(), "nothing follows the item");
            value
        })
        .map_err(|error| match failed(error) {
            IoError::Io(Ok(failed)) => IoError::Io(failed),
            IoError::Io(Err(kind)) => panic!("{kind:?}"),
            IoError::Cbor(error) => IoError::Cbor(error),
        });
    (read, reader.calls)
}

fn write_embedded(
    value: &Reading,
    script: Script,
    buf: usize,
) -> (Result<usize, IoError<Failed>>, Sink) {
    let mut writer = sink(script);
    let written = cbor_static::embedded_io::to_writer(value, &mut writer, &mut vec![0; buf]);
    (written, writer)
}

fn write_std(
    value: &Reading,
    script: Script,
    buf: usize,
) -> (Result<usize, IoError<Failed>>, Sink) {
    let mut writer = sink(script);
    let written =
        cbor_static::io::to_writer(value, &mut writer, &mut vec![0; buf]).map_err(|error| {
            match failed(error) {
                IoError::Io(Ok(failed)) => IoError::Io(failed),
                IoError::Io(Err(kind)) => panic!("{kind:?}"),
                IoError::Cbor(error) => IoError::Cbor(error),
            }
        });
    (written, writer)
}

type Read = fn(&[u8], Script, usize) -> (Result<Reading, IoError<Failed>>, usize);
type Write = fn(&Reading, Script, usize) -> (Result<usize, IoError<Failed>>, Sink);

const READS: [Read; 2] = [read_embedded, read_std];
const WRITES: [Write; 2] = [write_embedded, write_std];

fn steps() -> impl Iterator<Item = usize> {
    (1..=8).filter(|step| !cfg!(miri) || [1, 8].contains(step))
}

/// Every scratch buffer size by every step a mock moves per call.
fn cells() -> impl Iterator<Item = (usize, usize)> {
    buffer_sizes().flat_map(|buf| steps().map(move |step| (buf, step)))
}

fn buffer_sizes() -> impl Iterator<Item = usize> {
    (1..=64).filter(|size| !cfg!(miri) || [1, 64].contains(size))
}

fn kind_at<T, E>(result: &Result<T, IoError<E>>) -> Option<(ErrorKind, u32)> {
    match result {
        Err(IoError::Cbor(error)) => Some((error.kind, error.offset)),
        _ => None,
    }
}

#[test]
fn every_buffer_size_and_step_writes_and_reads_the_same_item() {
    let (value, bytes) = reading();
    for (read, write) in READS.into_iter().zip(WRITES) {
        for buf in buffer_sizes() {
            for step in steps() {
                let (written, writer) = write(&value, Script::steps(step), buf);
                assert_eq!(written, Ok(bytes.len()), "buf {buf} step {step}");
                assert_eq!(writer.out, bytes, "buf {buf} step {step}");
                assert_eq!(writer.flushes, 0, "buf {buf} step {step}");
                assert_eq!(read(&bytes, Script::steps(step), buf).0, Ok(reading().0));
            }
        }
    }
}

#[test]
fn a_reader_error_at_any_call_is_returned_as_io() {
    let (_, bytes) = reading();
    for read in READS {
        for (buf, step) in cells() {
            let (_, calls) = read(&bytes, Script::steps(step), buf);
            for fail_at in 0..calls {
                let script = Script {
                    fail_at: Some(fail_at),
                    ..Script::steps(step)
                };
                assert_eq!(
                    read(&bytes, script, buf).0,
                    Err(IoError::Io(Failed(fail_at))),
                    "buf {buf} step {step}"
                );
            }
        }
    }
}

#[test]
fn a_writer_error_at_any_call_is_returned_as_io() {
    let (value, _) = reading();
    for write in WRITES {
        for (buf, step) in cells() {
            let (_, writer) = write(&value, Script::steps(step), buf);
            for fail_at in 0..writer.calls {
                let script = Script {
                    fail_at: Some(fail_at),
                    ..Script::steps(step)
                };
                assert_eq!(
                    write(&value, script, buf).0,
                    Err(IoError::Io(Failed(fail_at))),
                    "buf {buf} step {step}"
                );
            }
        }
    }
}

#[test]
fn an_embedded_writer_that_takes_nothing_is_too_small_at_the_bytes_it_took() {
    let (value, _) = reading();
    for (buf, step) in cells() {
        let (_, writer) = write_embedded(&value, Script::steps(step), buf);
        for zero_at in 0..writer.calls {
            let script = Script {
                zero_at: Some(zero_at),
                ..Script::steps(step)
            };
            let (written, writer) = write_embedded(&value, script, buf);
            assert_eq!(
                kind_at(&written),
                Some((
                    ErrorKind::BufferTooSmall,
                    u32::try_from(writer.out.len()).expect("short")
                )),
                "buf {buf} step {step} zero at {zero_at}"
            );
        }
    }
}

#[test]
fn a_std_writer_that_takes_nothing_is_write_zero() {
    let (value, _) = reading();
    for (buf, step) in cells() {
        let (_, writer) = write_std(&value, Script::steps(step), buf);
        for zero_at in 0..writer.calls {
            let mut writer = sink(Script {
                zero_at: Some(zero_at),
                ..Script::steps(step)
            });
            assert!(
                matches!(
                    cbor_static::io::to_writer(&value, &mut writer, &mut vec![0; buf]),
                    Err(IoError::Io(error)) if error.kind() == std::io::ErrorKind::WriteZero
                ),
                "buf {buf} step {step} zero at {zero_at}"
            );
        }
    }
}

#[test]
fn a_stream_ending_at_any_byte_is_unexpected_end_there() {
    let (_, bytes) = reading();
    for read in READS {
        for (buf, step) in cells() {
            for cut in 0..bytes.len() {
                assert_eq!(
                    kind_at(&read(&bytes[..cut], Script::steps(step), buf).0),
                    Some((ErrorKind::UnexpectedEnd, u32::try_from(cut).expect("short"))),
                    "buf {buf} step {step} cut {cut}"
                );
            }
        }
    }
}

#[test]
fn an_empty_buffer_is_too_small_in_either_direction() {
    let (value, bytes) = reading();
    for read in READS {
        assert_eq!(
            kind_at(&read(&bytes, Script::steps(5), 0).0),
            Some((ErrorKind::BufferTooSmall, 0))
        );
    }
    for write in WRITES {
        assert_eq!(
            kind_at(&write(&value, Script::steps(5), 0).0),
            Some((ErrorKind::BufferTooSmall, 0))
        );
    }
}

#[test]
fn the_rest_of_the_last_read_is_the_next_item() {
    let mut reader = b"\x01\x02\x03".as_slice();
    let mut buf = [0u8; 2];
    let (first, rest) =
        cbor_static::embedded_io::from_reader::<u8, _>(&mut reader, &mut buf).expect("reads");
    assert_eq!((first, rest), (1, b"\x02".as_slice()));
    assert_eq!(reader, b"\x03");

    let mut reader = std::io::Cursor::new(b"\x01\x02\x03");
    let (first, rest) =
        cbor_static::io::from_reader::<u8, _>(&mut reader, &mut buf).expect("reads");
    assert_eq!((first, rest), (1, b"\x02".as_slice()));
    assert_eq!(reader.position(), 2);
}

#[test]
fn a_std_read_interrupted_is_retried() {
    struct Interrupting<'a> {
        bytes: &'a [u8],
        interrupt: bool,
    }

    impl std::io::Read for Interrupting<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.interrupt = !self.interrupt;
            if self.interrupt {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            std::io::Read::read(&mut self.bytes, buf)
        }
    }

    let (_, bytes) = reading();
    let mut reader = Interrupting {
        bytes: &bytes,
        interrupt: false,
    };
    assert_eq!(
        cbor_static::io::from_reader::<Reading, _>(&mut reader, &mut [0; 4])
            .map(|(value, _)| value)
            .ok(),
        Some(reading().0)
    );
}
