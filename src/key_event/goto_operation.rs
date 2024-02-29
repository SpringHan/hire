// Goto Operation.

use crate::app::{App, OptionFor};
use crate::app::command::OperationError;
use super::Goto;

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::error::Error;

use toml_edit::{Document, value};

pub fn goto_operation(app: &mut App,
                      key: char,
                      in_root: bool
) -> Result<(), Box<dyn Error>>
{
    if key == 'g' {
        super::cursor_movement::move_cursor(app, Goto::Index(0), in_root)?;
        app.option_key = OptionFor::None;
        return Ok(())
    }

    // TODO: Match `key` with app.target_dir
    let mut config_file = get_config_file()?;
    let mut config = String::new();
    config_file.read_to_string(&mut config)?;

    // Pressed invalid key.
    if config.is_empty() {
        return Ok(())
    }

    let mut toml_config: Document = config.parse()
        .expect("Failed to read config from toml file!");

    // match key {
    //     'h' => app.goto_dir("/home/spring")?,
    //     '/' => app.goto_dir("/")?,
    //     'G' => app.goto_dir("/home/spring/Github")?,
    //     _ => ()
    // }

    app.option_key = OptionFor::None;

    Ok(())
}

fn add_target_dir(app: &mut App, key: char, path: String) -> io::Result<()> {
    let mut file_open = OpenOptions::new();

    let mut read_file = file_open.read(true).open(&app.config_path)?;
    let mut config = String::new();
    read_file.read_to_string(&mut config)?;

    let mut toml_config: Document = if config.is_empty() {
        Document::new()
    } else {
        config.parse().expect("Failed to read config from toml file!")
    };
    toml_config["target_dir"][String::from(key)] = value(&path);

    let mut write_file = file_open
        .write(true)
        .truncate(true)
        .open(&app.config_path)?;
    write_file.write_all(toml_config.to_string().as_bytes())?;

    app.target_dir.entry(key).or_insert(path);

    Ok(())
}

fn remove_target_dir(app: &mut App, key: char) -> io::Result<()> {
    let mut file_open = OpenOptions::new();

    let mut read_file = file_open.read(true).open(&app.config_path)?;
    let mut config = String::new();
    read_file.read_to_string(&mut config)?;

    if config.is_empty() {
        OperationError::NotFound(None).check(app);
        return Ok(())
    }

    let mut toml_config: Document = config
        .parse()
        .expect("Failed to read config from toml file!");
    toml_config["target_dir"][&String::from(key)] = toml_edit::Item::None;
    
    let mut write_file = file_open
        .write(true)
        .truncate(true)
        .open(&app.config_path)?;
    write_file.write_all(toml_config.to_string().as_bytes())?;

    app.target_dir.remove(&key);

    Ok(())
}

// TODO: To be removed.
fn get_config_file() -> io::Result<File> {
    todo!()
}

/// Create config file if it doesn't exist.
///
/// Then return the file path.
pub fn create_config_file() -> io::Result<String> {
    let user = std::env::var("USER")
        .expect("Failed to get user name!");
    let config_dir = format!(
        "{}/.config/springhan/hire/",
        if user == String::from("root") {
            String::from("/root")
        } else {
            format!("/home/{}", user)
        }
    );
    let file_path = format!("{}config.toml", config_dir);
    
    if let Err(err) = File::open(&config_dir) {
        if err.kind() != ErrorKind::NotFound {
            return Err(err)
        }

        fs::create_dir_all(&config_dir)?;
        File::create(&file_path)?;
    }

    Ok(file_path)
}

#[cfg(test)]
mod goto_test {
    use super::*;

    #[test]
    fn test_name() {
        let mut app = App::default();
        add_target_dir(&mut app, 'a', "/aaa/bbb/ccc".to_owned())
            .unwrap();
        println!("{:?}", app.target_dir);
        panic!()
    }

    #[test]
    fn test_b() {
        let mut app = App::default();
        remove_target_dir(&mut app, 'a').unwrap();
    }
}
