// Types for config

use std::borrow::Cow;

use toml_edit::Item;
use anyhow::{bail, Result};

pub type AppConfig = Vec<Config>;

#[derive(Clone)]
pub enum ConfigValue {
    // Bool(bool),
    String(Cow<'static, str>),
    Vec(Vec<Cow<'static, str>>),
    // HashMap(HashMap<char, String>),
}

pub struct Config {
    name: String,
    value: ConfigValue
}

// Config Implements
impl Config {
    fn default_value(prop: &str) -> ConfigValue {
        match prop {
            "gui_commands" => ConfigValue::Vec(Vec::new()),
            "default_shell" => ConfigValue::String(Cow::Owned(String::from("bash"))),
            _ => panic!("Unknow error occurred at default_value fn in types.rs.")
        }
    }

    pub fn get_value<'a>(configs: &'a Vec<Self>, prop: &str) -> &'a ConfigValue {
        for conf in configs.iter() {
            if &conf.name == prop {
                return &conf.value
            }
        }

        // NOTE: There's no possibility that this function cannot find value.
        // The only cause is typo error.
        panic!("Error in code at get_value fn in config/types.rs!")
    }

    pub fn generate_default(prop: &str) -> Self {
        Self {
            name: prop.to_owned(),
            value: Self::default_value(prop)
        }
    }

    pub fn value_from(&mut self, value: &Item) -> Result<()> {
        let err_msg = format!("The type of config property {} is error", self.name);

        match self.name.as_str() {
            "default_shell" => {
                let _value = value.as_str();
                if _value.is_none() {
                    bail!("{err_msg}")
                }

                self.value = ConfigValue::String(Cow::Owned(
                    _value.unwrap().to_owned()
                ));
            },

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
}
