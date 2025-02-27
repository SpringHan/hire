// App Result

use std::{env, io};
use std::convert::From;

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub struct AppError {
    errors: Vec<ErrorType>
}

/// The three cases matching not found error.
#[derive(Debug, Error)]
pub enum NotFoundType {
    #[error("[Error]: Not found: {0}")]
    Item(String),

    #[error("[Error]: The file/item cannot be found!")]
    None
}

/// This enum is used for the errors that will not destroy program.
#[derive(Debug, Error)]
pub enum ErrorType {
    #[error("[AppError]: Permission Denied: {0:?}")]
    PermissionDenied(Vec<String>),

    #[error("[AppError]: Invalid Command!")]
    UnvalidCommand,

    #[error("[AppError]: File exists: {0:?}")]
    FileExists(Vec<String>),

    #[error("[AppError]: No item to be selected and operated!")]
    NoSelected,

    #[error(transparent)]
    NotFound(#[from] NotFoundType),

    #[error("[AppError]: {0}!")]
    Specific(String),

    #[error("[AppError/IoError]: {{ 0.to_string() }}")]
    Io(#[from] io::Error),

    #[error("[AppError/VarError]: {0}")]
    Var(#[from] env::VarError),

    #[error("[AppError]: {0:?}")]
    Others(#[from] anyhow::Error)
}

// AppError Implements
impl AppError {
    pub fn new() -> Self {
        AppError { errors: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn iter(self) -> impl Iterator<Item = ErrorType> {
        self.errors.into_iter()
    }

    pub fn add_error<E: Into<ErrorType>>(&mut self, error: E) {
        self.errors.push(error.into());
    }

    pub fn append_errors<I>(&mut self, errors: I)
    where I: Iterator<Item = ErrorType>
    {
        self.errors.extend(errors);
    }

    pub fn clear(&mut self) {
        self.errors.clear();
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut final_msg: Vec<String> = Vec::new();

        for err in self.errors.iter() {
            final_msg.push(err.to_string());
        }

        write!(f, "{}", final_msg.join("\n"))
    }
}

impl From<io::Error> for AppError {
    fn from(value: io::Error) -> Self {
        Self {
            errors: vec![
                value.into()
            ]
        }
    }
}

impl From<env::VarError> for AppError {
    fn from(value: env::VarError) -> Self {
        Self {
            errors: vec![
                value.into()
            ]
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        Self {
            errors: vec![
                value.into()
            ]
        }
    }
}

// ErrorType Implements
impl ErrorType {
    pub fn pack(self) -> AppError {
        AppError { errors: vec![self] }
    }
}
