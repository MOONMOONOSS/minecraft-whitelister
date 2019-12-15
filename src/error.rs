use std::{error::Error as StdError, fmt};

#[derive(Debug)]
pub enum WhitelistErrorKind {
  NonExistingPlayer,
  RCONConnectionError,
  Other,
}

pub trait WhitelistErrorInfo {
  fn message(&self) -> &str;
}

impl fmt::Debug for dyn WhitelistErrorInfo + Send + Sync {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.message(), f)
    }
}

impl WhitelistErrorInfo for String {
    fn message(&self) -> &str {
        self
    }
}

#[derive(Debug)]
pub enum Error {
  WhitelistError(
    WhitelistErrorKind,
    Box<dyn WhitelistErrorInfo + Send + Sync>, // Message
  ),
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match *self {
      Error::WhitelistError(_, ref e) => write!(f, "{}", e.message()),
    }
  }
}

impl StdError for Error {
  fn description(&self) -> &str {
    match *self {
      Error::WhitelistError(_, ref e) => e.message(),
    }
  }
}