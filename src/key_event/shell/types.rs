// Types

use crate::{app::App, error::{AppResult, ErrorType}};

#[derive(Clone, Copy)]
pub enum CommandStr<'a> {
    /// The SelectedItem can only be used for commands read from keymap.toml.
    SelectedItem,
    Str(&'a str)
}

pub enum ShellCommand<'a> {
    Shell,

    /// The first element is the shell that command runs on;
    /// The second one is the program & its arguments.
    Command(
        Option<&'a str>,
        Vec<CommandStr<'a>>
    )
}

impl<'a> From<&'a str> for CommandStr<'a> {
    fn from(value: &'a str) -> Self {
        if value == "$." {
            return Self::SelectedItem
        }

        Self::Str(value)
    }
}

// NOTE: This method can only be called when getting program name,
// which cannot be the selected item.
impl<'a> Into<&'a str> for CommandStr<'a> {
    fn into(self) -> &'a str {
        match self {
            CommandStr::Str(_str) => _str,
            CommandStr::SelectedItem => panic!("Unknow error occurred when CommandStr -> &str"),
        }
    }
}

impl<'a> CommandStr<'a> {
    pub fn from_strs(str_vec: Vec<&'a str>) -> Vec<Self> {
        str_vec.into_iter()
            .map(|_str| Self::Str(_str))
            .collect::<Vec<_>>()
    }

    /// The `join` function for command read from keymap.toml.
    pub fn join_from_keymap(str_vec: Vec<Self>, app: &App) -> AppResult<String> {
        let selected_item = app.get_file_saver();

        let mut first = true;
        let mut joined_str = String::new();

        for e in str_vec.into_iter() {
            if first {
                first = false;
            } else {
                joined_str.push(' ');
            }

            match e {
                CommandStr::Str(_str) => joined_str.push_str(_str),
                CommandStr::SelectedItem => {
                    if let Some(item) = selected_item {
                        joined_str.push_str(&item.name);
                    } else {
                        return Err(ErrorType::NoSelected.pack())
                    }
                },
            }
        }

        Ok(joined_str)
    }

    /// Convert a CommandStr Vec into a &str Vec.
    pub fn str_vec(str_vec: Vec<Self>) -> Vec<&'a str> {
        str_vec.into_iter()
            .map(|e| e.into())
            .collect::<Vec<_>>()
    }
}
