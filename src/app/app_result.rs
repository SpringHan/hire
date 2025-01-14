// App Result

use std::io;
use std::convert::From;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub struct AppError {
    errors: Vec<ErrorType>
}

/// The three cases matching not found error.
#[derive(Debug)]
pub enum NotFoundType {
    // Files(Vec<String>),
    Item(String),
    None
}

/// This enum is used for the errors that will not destroy program.
#[derive(Debug)]
pub enum ErrorType {
    PermissionDenied(Option<Vec<String>>),
    UnvalidCommand,
    FileExists(Vec<String>),
    NoSelected,
    NotFound(NotFoundType),
    Specific(String),

    #[allow(unused)]
    Io(io::ErrorKind, String)
}

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
            final_msg.push(err.error_value());
        }

        write!(f, "{}", final_msg.join("\n"))
    }
}

impl From<io::Error> for AppError {
    fn from(value: io::Error) -> Self {
        AppError {
            errors: vec![
                value.into()
            ]
        }
    }
}

impl From<io::Error> for ErrorType {
    fn from(value: io::Error) -> Self {
        ErrorType::Io(value.kind(), value.to_string())
    }
}

impl ErrorType {
    pub fn error_value(&self) -> String {
        match self {
            ErrorType::NoSelected => {
                String::from("[Error]: No item to be selected and operated!")
            },
            ErrorType::FileExists(files) => {
                format!("[Error]: File {:?} already exists!", files)
            },
            ErrorType::UnvalidCommand => {
                String::from("[Error]: Unvalid Command!")
            },
            ErrorType::PermissionDenied(files) => {
                if let Some(files) = files {
                    format!("[Error]: Permission Denied: {:?}", files)
                } else {
                    String::from("[Error]: Permission Denied!")
                }                
            },
            ErrorType::NotFound(data) => {
                match data {
                    // NotFoundType::Files(files) => format!("[Error]: Not found: {:?}", files),
                    NotFoundType::Item(item) => format!("[Error]: Not found: {}", item),
                    NotFoundType::None => String::from("[Error]: The file/item cannot be found!")
                }
            },
            ErrorType::Specific(err) => {
                format!("[Error]: {}", err)
            }
            ErrorType::Io(_, msg) => msg.to_owned()
        }
    }

    pub fn pack(self) -> AppError {
        AppError { errors: vec![self] }
    }
}
