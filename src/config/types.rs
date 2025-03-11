// Types for config

use std::borrow::Cow;

use toml_edit::Item;
use anyhow::{bail, Result};

use crate::option_get;

pub type AppConfig<'a> = Vec<Config<'a>>;

#[derive(Clone)]
pub enum ConfigValue<'a> {
    // Bool(bool),
    String(Cow<'a, str>),
    Vec(Vec<Cow<'a, str>>),
    // HashMap(HashMap<char, String>),
}

pub struct Config<'a> {
    name: String,
    value: ConfigValue<'a>
}

// Config Implements
impl<'a> Config<'a> {
    fn default_value(prop: &str) -> ConfigValue {
        match prop {
            "gui_commands" => ConfigValue::Vec(Vec::new()),
            "default_shell" => ConfigValue::String(Cow::Borrowed("bash")),
            "file_read_program" => ConfigValue::String(Cow::Borrowed("vim")),
            "file_operation_editor" => ConfigValue::String(Cow::Borrowed("vim")),
            _ => panic!("Unknow error occurred at default_value fn in types.rs.")
        }
    }

    pub fn get_value(configs: &'a Vec<Self>, prop: &str) -> &'a ConfigValue<'a> {
        for conf in configs.iter() {
            if &conf.name == prop {
                return &conf.value
            }
        }

        // NOTE: There's no possibility that this function cannot find value.
        // The only cause is typo error.
        panic!("Error in code at get_value fn in config/types.rs!")
    }

    pub fn generate_default(prop: &'a str) -> Self {
        Self {
            name: prop.to_owned(),
            value: Self::default_value(prop)
        }
    }

    pub fn value_from(&mut self, value: &Item) -> Result<()> {
        let err_msg = format!("The type of config property {} is error", self.name);

        match self.name.as_str() {
            "default_shell" | "file_read_program" | "file_operation_editor" =>
                self.value = Self::get_str(value, err_msg)?,

            "gui_commands" => {
                let _value = value.as_array();
                if _value.is_none() {
                    bail!("{err_msg}")
                }

                let mut command: Option<&str>;

                if let ConfigValue::Vec(ref mut commands) = self.value {
                    for _command in _value.unwrap().iter() {
                        command = _command.as_str();

                        if command.is_none() {
                            bail!("Meet type error when setting gui_commands")
                        }

                        commands.push(Cow::Owned(command.unwrap().to_owned()));
                    }
                }
            },

            _ => panic!("Unknow error occurred at value_from fn in types.rs.")
        }

        Ok(())
    }

    fn get_str(value: &Item, err_msg: String) -> Result<ConfigValue<'a>> {
        let _str = option_get!(value.as_str(), err_msg);

        Ok(ConfigValue::String(Cow::Owned(_str.to_owned())))
    }
}
