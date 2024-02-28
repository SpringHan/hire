// Goto Operation.

use crate::app::{App, OptionFor};
use super::Goto;

use std::fs::File;
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
    let mut file = get_config_file()?;
    let mut config = String::new();
    file.read_to_string(&mut config)?;

    let mut toml_config: Document = if config.is_empty() {
        Document::new()
    } else {
        config.parse().expect("Failed to read config from toml file!")
    };

    // if config.is_empty() {
    //     let mut toml_config = Document::new();
    //     toml_config["target_dir"][String::from(key)] = value(&path);
    // } else {
    //     let mut toml_config: Document = config
    //         .parse()
    //         .expect("Failed to read config from toml file!");
    //     toml_config["target_dir"][String::from(key)] = value(&path);
    // }
    // TODO: Replace these codes with OpenOption.
    toml_config["target_dir"][String::from(key)] = value(&path);
    file.write_all(toml_config.to_string().as_bytes())?;

    app.target_dir.entry(key).or_insert(path);

    Ok(())
}

fn remove_target_dir(key: char, path: String) -> io::Result<()> {
    todo!()
}

fn get_config_file() -> io::Result<File> {
    let user = std::env::var("USER")
        .expect("Failed to get user name!");
    let file_path = format!(
        "{}/.config/springhan/hire/config.toml",
        if user == String::from("root") {
            String::from("/root")
        } else {
            format!("/home/{}", user)
        }
    );

    let file = File::open(&file_path);
    match file {
        Ok(f) => Ok(f),
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                let new_created = File::create(file_path)
                    .expect("Failed to create HIRE config file.");
                return Ok(new_created)
            }

            Err(err)
        }
    }
}
