// App Result

use crate::app::App;

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
    Files(Vec<String>),
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
    Io(io::ErrorKind, String),
    Fatal(String)
}

impl AppError {
    pub fn new() -> Self {
        AppError { errors: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn get_iter<'a>(&'a self) -> impl Iterator<Item = &'a ErrorType> {
        self.errors.iter()
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
                    NotFoundType::Files(files) => format!("[Error]: Not found: {:?}", files),
                    NotFoundType::Item(item) => format!("[Error]: Not found: {}", item),
                    NotFoundType::None => String::from("[Error]: The file/item cannot be found!"),
                }
            },
            ErrorType::Specific(err) => {
                format!("[Error]: {}", err)
            }
            ErrorType::Io(_, msg) => msg.to_owned(),
            ErrorType::Fatal(err) => panic!("{}", err),
        }
    }

    pub fn pack(self) -> AppError {
        AppError { errors: vec![self] }
    }

    /// Check whether the OperationError is None
    /// If it's None, return true. If previous errors exist, still return false.
    pub fn check(self, app: &mut App) -> bool {
        // if let Some(msg) = self.error_value() {
        //     if let
        //         Block::CommandLine(
        //             ref mut error,
        //             ref mut cursor
        //         ) = app.selected_block
        //     {
        //         if app.command_error {
        //             error.push_str(&format!("\n{}", msg));
        //         } else {
        //             *error = msg;
        //             *cursor = CursorPos::None;
        //             app.command_error = true;
        //         }

        //         // Turn off Switch mode.
        //         if let super::OptionFor::Switch(_) = app.option_key {
        //             app.option_key = super::OptionFor::None;
        //         }
        //     } else {
        //         app.set_command_line(msg, CursorPos::None);
        //         app.command_error = true;
        //     }

        //     return false
        // }

        // // Though current error not exists, but previous errors exist.
        // if app.command_error {
        //     return false
        // }

        true
    }
}
