use std::fmt;
use std::io::{self, Write, Read};

pub struct HexDump<'a>(&'a [u8]);

impl<'a> fmt::Display for HexDump<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.iter()
            .map(|byte| format!("{:02X}", byte))
            .collect::<Vec<_>>()
            .join(" "))
    }
}

pub struct IoDebug<C: Read + Write>(pub C);

impl<C: Read + Write> Read for IoDebug<C> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf) {
            Ok(bytes) => {
                println!("< {}", HexDump(&buf[0..bytes]));
                Ok(bytes)
            }
            Err(e) => Err(e)
        }
    }
}

impl<C: Read + Write> Write for IoDebug<C> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.0.write(buf) {
            Ok(bytes) => {
                println!("> {}", HexDump(&buf[0..bytes]));
                Ok(bytes)
            }
            Err(e) => Err(e),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
