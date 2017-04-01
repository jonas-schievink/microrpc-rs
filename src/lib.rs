//! A Rust implementation of the µRPC protocol (client-side).

extern crate byteorder;

mod error;

pub use error::Error;

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};

use std::io::prelude::*;
use std::io;
use std::fmt;

/// Version byte indicating the µRPC protocol version implemented by this library.
const VERSION: u8 = 0;

/// Result type returned by many functions of this library.
pub type Result<T> = std::result::Result<T, Error>;

#[repr(u8)]
enum Request {
    Version = 0,
    Enumerate = 1,
    Call = 2,
}

/// A client connected to a remote µRPC Server.
pub struct Client<C: Write + Read> {
    channel: C,
    procedures: Option<Box<[Procedure]>>,
}

impl<C: Write + Read> Client<C> {
    /// Creates a new `Client` object using `channel` for communication with the µRPC server.
    pub fn new(channel: C) -> Self {
        Client {
            channel: channel,
            procedures: None,
        }
    }

    /// Re-Enumerate the Server's exported procedures and store them.
    ///
    /// Since this will be called automatically before anything else is communicated, this also
    /// checks that the Server's protocol version matches.
    pub fn enumerate(&mut self) -> Result<&[Procedure]> {
        // Check version
        self.channel.write_u8(Request::Version as u8)?;
        self.read_success()?;
        let server_version = self.channel.read_u8()?;
        if server_version != VERSION {
            return Err(Error::MismatchedVersion {
                ours: VERSION,
                theirs: server_version,
            });
        }

        // Enumerate
        self.channel.write_u8(Request::Enumerate as u8)?;
        self.read_success()?;
        let num_procs = self.channel.read_u16::<NetworkEndian>()?;
        let mut procs = Vec::with_capacity(num_procs as usize);
        for i in 0..num_procs {
            // Read procedure descriptors
            let byte0 = self.channel.read_u8()?;
            let has_return_value = byte0 & 0x80 != 0;
            let num_params = byte0 & 0x7f;

            let return_type = if has_return_value {
                Some(Type::read(&mut self.channel)?)
            } else {
                None
            };

            let mut params = Vec::with_capacity(num_params as usize);
            for _ in 0..num_params {
                params.push(Type::read(&mut self.channel)?);
            }

            procs.push(Procedure {
                id: i,
                parameters: params.into_boxed_slice(),
                returns: return_type,
            });
        }

        self.procedures = Some(procs.into_boxed_slice());

        Ok(self.procedures.as_ref().map(|procs| &**procs).unwrap())
    }

    /// Get the list of exported procedures.
    ///
    /// If `enumerate` was already called, immediately returns the cached list. Otherwise, calls
    /// `enumerate` and returns its result.
    pub fn procedures(&mut self) -> Result<&[Procedure]> {
        if let Some(ref procs) = self.procedures {
            return Ok(procs);
        }

        self.enumerate()
    }

    /// Calls a procedure.
    pub fn call(&mut self, id: u16, arguments: &[Value]) -> Result<Option<Value>> {
        {
            let procs = self.procedures()?;

            if id as usize >= procs.len() {
                return Err(Error::ProcOutOfRange);
            }

            let procedure = &procs[id as usize];
            assert_eq!(procedure.id, id);

            // Make sure all arguments match
            for (i, (got, expected)) in arguments.iter().zip(procedure.parameters.iter()).enumerate() {
                if got.ty() != *expected {
                    assert!(i < 256);
                    return Err(Error::MismatchedArguments {
                        index: i as u8,
                        expected: *expected,
                        found: got.ty(),
                    });
                }
            }
        }

        self.channel.write_u8(Request::Call as u8)?;
        self.channel.write_u16::<NetworkEndian>(id)?;
        for arg in arguments {
            arg.write(&mut self.channel)?;
        }

        // Read server response
        self.read_result()?;

        if let Some(ret_ty) = self.procedures()?[id as usize].returns {
            let retval = Value::read(&ret_ty, &mut self.channel)?;
            Ok(Some(retval))
        } else {
            Ok(None)
        }
    }

    /// Reads a result byte from the Server.
    ///
    /// If the result byte indicates success, returns `Ok(())`. Otherwise, returns an appropriate
    /// error. If an I/O error occurs, also returns an error.
    fn read_result(&mut self) -> Result<()> {
        match self.channel.read_u8()? {
            0x00 => Ok(()),
            0x01 => Err(Error::GenericError),
            _ => Err(Error::ProtocolError {
                description: "invalid result byte",
            }),
        }
    }

    /// Reads a result byte from the server, requiring it to be a success value.
    ///
    /// µRPC specifies that some requests can not fail and must always report a `Success` result. If
    /// those do fail, the implementation is incorrect.
    ///
    /// If the result byte indicates success, returns `Ok(())`. Otherwise, returns a protocol error.
    /// If an I/O error occurs, also returns an error.
    fn read_success(&mut self) -> Result<()> {
        match self.channel.read_u8()? {
            0x00 => Ok(()),
            _ => Err(Error::ProtocolError {
                description: "invalid result of infallible request",
            }),
        }
    }
}

/// A callable procedure exported by a µRPC server.
///
/// You cannot use this type to directly call a procedure exported by a server. To do that, use
/// `Client::call`.
pub struct Procedure {
    id: u16,
    parameters: Box<[Type]>,
    returns: Option<Type>,
}

impl Procedure {
    /// Gets the ID of the procedure used to call it.
    pub fn id(&self) -> u16 { self.id }

    /// Gets the types of the parameters this procedure expects.
    pub fn parameter_types(&self) -> &[Type] { &self.parameters }

    /// Gets the type of the return value of this procedure (if there is a return value).
    pub fn return_type(&self) -> Option<Type> { self.returns }
}

/// Types supported by µRPC.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Type {
    /// 8-bit integer.
    U8,
    /// 16-bit integer.
    U16,
}

impl Type {
    /// Reads a `Type` encoded as specified in the µRPC protocol.
    fn read<R: Read>(r: &mut R) -> Result<Self> {
        Ok(match r.read_u8()? {
            0x00 => Type::U8,
            0x01 => Type::U16,
            _ => return Err(Error::ProtocolError {
                description: "invalid type",
            }),
        })
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
        }
    }
}

/// µRPC values, returned by procedures and passed as arguments.
pub enum Value {
    /// 8-bit integer.
    U8(u8),
    /// 16-bit integer.
    U16(u16),
}

impl Value {
    /// Reads a `Value` of given `Type` from a reader.
    fn read<R: Read>(ty: &Type, r: &mut R) -> io::Result<Self> {
        Ok(match *ty {
            Type::U8 => {
                let mut buf = [0];
                r.read_exact(&mut buf)?;
                Value::U8(buf[0])
            }
            Type::U16 => {
                let mut buf = [0, 0];
                r.read_exact(&mut buf)?;

                let (msb, lsb) = (buf[0] as u16, buf[1] as u16);
                Value::U16(msb << 8 | lsb)
            }
        })
    }

    /// Writes this `Value` for transmission according to the µRPC protocol.
    fn write<W: Write>(&self, w: &mut W) -> io::Result<()> {
        match *self {
            Value::U8(val) => w.write_u8(val),
            Value::U16(val) => w.write_u16::<NetworkEndian>(val),
        }
    }

    /// Gets the type of this `Value`.
    fn ty(&self) -> Type {
        match *self {
            Value::U8(_) => Type::U8,
            Value::U16(_) => Type::U16,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Value::U8(i) => write!(f, "{}", i),
            Value::U16(i) => write!(f, "{}", i),
        }
    }
}
