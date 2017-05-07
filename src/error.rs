use std::io;
use std::fmt;
use std::error;

// TODO: Make this a struct containing an enum (quick_error? error_chain?)

/// Errors that can occur when attempting to perform a µRPC operation.
#[derive(Debug)]
pub enum Error {
    IoError(io::Error),

    /// Server returned error code 1, generic error.
    ///
    /// This can occur when calling a procedure with mismatched parameters. Note that all errors
    /// below are signaled by this library, not the Server.
    GenericError,

    /// Procedure ID is out of range.
    ProcOutOfRange,

    /// An incorrect number of arguments was passed to `Client::call`.
    MismatchedArgumentCount {
        /// Expected number of arguments.
        expected: u16,
        /// Actual number of arguments.
        found: usize,
    },

    /// Argument types are mismatched.
    MismatchedArguments {
        /// Index of mismatched argument.
        index: u8,
        /// Type specified by the Server.
        expected: ::Type,
        /// Type provided by the user.
        found: ::Type,
    },

    /// µRPC protocol version mismatch.
    MismatchedVersion {
        ours: u8,
        theirs: u8,
    },

    /// Server responded with an invalid value which is not allowed according to the µRPC protocol.
    ProtocolError {
        description: &'static str,
    },
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::IoError(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::IoError(ref io) => writeln!(f, "i/o error: {}", io),
            Error::GenericError => writeln!(f, "generic µRPC error"),
            Error::ProcOutOfRange => writeln!(f, "procedure id out of range"),
            Error::MismatchedArgumentCount { expected, found } =>
                writeln!(f, "mismatched number of arguments (expected {}, got {})",
                         expected, found),
            Error::MismatchedArguments { index, ref expected, ref found } =>
                writeln!(f, "mismatched argument type for argument index {} (expected {}, found \
                             {})", index, expected, found),
            Error::MismatchedVersion { ours, theirs } =>
                writeln!(f, "mismatched µRPC version (this library implements version {}, the \
                             other endpoint implements {})", ours, theirs),
            Error::ProtocolError { description } =>
                writeln!(f, "protocol error: {}", description),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::IoError(ref io) => io.description(),
            Error::GenericError => "generic µRPC error",
            Error::ProcOutOfRange => "procedure id out of range",
            Error::MismatchedArgumentCount { .. } => "mismatched number of arguments",
            Error::MismatchedArguments { .. } => "mismatched arguments",
            Error::MismatchedVersion { .. } => "mismatched µRPC version",
            Error::ProtocolError { description } => description,
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::IoError(ref io) => Some(io),
            _ => None,
        }
    }
}
