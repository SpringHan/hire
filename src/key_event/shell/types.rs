// Types

use crate::{app::App, error::{AppResult, ErrorType}};

#[derive(Clone, Copy)]
pub enum CommandStr<'a> {
    /// The SelectedItem can only be used for commands read from keymap.toml.
    SelectedItem,
    /// A word written by the user. It is inserted into the command as it is,
    /// because it may be shell syntax (`|`, `>`, `$HOME`, quotes, …).
    Str(&'a str),
    /// One argument that must reach the program intact, such as a file path.
    /// It is quoted when needed while the words are joined, so that a space,
    /// `$` or `'` in it can neither split nor change the argument.
    Quoted(&'a str)
}

/// A one-shot command run in the foreground.
///
/// Interactive shells are `ShellSession`s instead.
pub enum ShellCommand<'a> {
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
            CommandStr::Str(_str) | CommandStr::Quoted(_str) => _str,
            CommandStr::SelectedItem => panic!("Unknow error occurred when CommandStr -> &str"),
        }
    }
}

/// Whether a shell would read `value` as anything other than one literal word,
/// which is the case when it is empty or contains a character the shell treats
/// specially.
fn needs_quoting(value: &str) -> bool {
    value.is_empty() ||
        value.chars().any(|chr| matches!(
            chr,
            ' ' | '\t' | '\n' | '\'' | '"' | '`' | '\\' | '$' | '*' | '?' |
            '[' | ']' | '~' | '!' | '#' | '&' | ';' | '|' | '<' | '>' |
            '(' | ')' | '{' | '}' | '='
        ))
}

/// Quote `value` so that a POSIX shell reads it as a single literal word, and
/// return it unchanged when it already is one.
///
/// Everything inside single quotes is literal, so a space, `$`, `*` or a
/// newline in a file name stays part of the word. A `'` cannot appear inside
/// them, so it closes the quotes, is escaped outside of them and the quotes
/// are reopened: `'` becomes `'\''`, which the shell reads as one `'`.
pub fn shell_quote(value: &str) -> String {
    if !needs_quoting(value) {
        return value.to_owned()
    }

    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');

    for chr in value.chars() {
        if chr == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(chr);
        }
    }

    quoted.push('\'');
    quoted
}

impl<'a> CommandStr<'a> {
    pub fn from_strs(str_vec: Vec<&'a str>) -> Vec<Self> {
        str_vec.into_iter()
            .map(|_str| Self::Str(_str))
            .collect::<Vec<_>>()
    }

    /// Read the words of a command line, where `$.` stands for the selected
    /// item. Both `:!` and `shell_command` in `keymap.toml` accept it.
    pub fn from_slices(str_vec: Vec<&'a str>) -> Vec<Self> {
        str_vec.into_iter()
            .map(|_str| if _str == "$." {
                Self::SelectedItem
            } else {
                Self::Str(_str)
            })
            .collect::<Vec<_>>()
    }

    /// Join the words into the command string `shell -c` runs.
    ///
    /// The result is shell source: `Str` words are kept as written, while the
    /// selected item and `Quoted` arguments are quoted when they need it, so a
    /// file name with a space is still a single argument when the shell runs
    /// the command.
    pub fn join_command(str_vec: Vec<Self>, app: &App) -> AppResult<String> {
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
                CommandStr::Quoted(_str) => {
                    joined_str.push_str(&shell_quote(_str))
                },
                CommandStr::SelectedItem => {
                    if let Some(item) = selected_item {
                        joined_str.push_str(&shell_quote(&item.name));
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

#[cfg(test)]
mod test {
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-shell-types-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_shell_quote() {
        // A plain word needs no quoting and is kept readable, which matters
        // when the result is shown on the command line.
        assert_eq!(shell_quote("a.txt"), "a.txt");
        assert_eq!(shell_quote("a b.txt"), "'a b.txt'");
        assert_eq!(shell_quote(""), "''");

        // Nothing is expanded or globbed inside the quotes.
        assert_eq!(shell_quote("$HOME *.txt"), "'$HOME *.txt'");

        // A quote is closed, escaped outside and reopened.
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    /// The quoted value has to reach the program as exactly one argument.
    #[test]
    fn test_quoted_value_is_one_argument() {
        let name = "a b '$HOME' *";
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("printf '<%s>' {}", shell_quote(name)))
            .output()
            .unwrap();

        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("<{name}>")
        );
    }

    /// `$.` is the selected item, and it is quoted, so the shell sees one
    /// argument even when the name contains a space.
    #[test]
    fn test_join_quotes_selected_item() {
        let dir = temp_dir("join");
        fs::write(dir.join("a b.txt"), "text").unwrap();

        let mut app = App::default();
        app.goto_dir(&dir, Some(false)).unwrap();

        let joined = CommandStr::join_command(
            vec![CommandStr::Str("vim"), CommandStr::SelectedItem],
            &app
        ).unwrap();

        assert_eq!(joined, "vim 'a b.txt'");

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A `Quoted` argument is quoted as well, while a `Str` word stays as the
    /// user wrote it.
    #[test]
    fn test_join_quotes_quoted_arguments_only() {
        let app = App::default();

        let joined = CommandStr::join_command(
            vec![
                CommandStr::Str("ls"),
                CommandStr::Str("-l"),
                CommandStr::Str("|"),
                CommandStr::Str("wc"),
                CommandStr::Quoted("/tmp/a b.txt"),
            ],
            &app
        ).unwrap();

        assert_eq!(joined, "ls -l | wc '/tmp/a b.txt'");
    }

    #[test]
    fn test_from_slices_expands_selected_item() {
        let words = CommandStr::from_slices(vec!["vim", "$.", "$HOME"]);

        assert!(matches!(words[0], CommandStr::Str("vim")));
        assert!(matches!(words[1], CommandStr::SelectedItem));
        // Only the whole word is the placeholder.
        assert!(matches!(words[2], CommandStr::Str("$HOME")));
    }
}
