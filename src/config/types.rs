// Types for config

use std::collections::HashMap;

use anyhow::{bail, Result};
use toml_edit::Item;

pub type AppConfig = Vec<Config>;

pub enum ConfigValue {
    Bool(bool),
    String(String),
    HashMap(HashMap<char, String>),
}

pub struct Config {
    name: String,
    value: ConfigValue
}

// TryInto traits for ConfigValue
impl TryInto<String> for &ConfigValue {
    type Error = anyhow::Error;

    fn try_into(self) -> std::result::Result<String, Self::Error> {
        if let ConfigValue::String(_str) = self {
            return Ok(_str.to_owned())
        }

        bail!("Failed to extract a non-string value from ConfigValue")
    }
}

impl TryInto<bool> for &ConfigValue {
    type Error = anyhow::Error;

    fn try_into(self) -> std::result::Result<bool, Self::Error> {
        if let ConfigValue::Bool(_bool) = self {
            return Ok(*_bool)
        }

        bail!("Failed to extract a non-boolean value from ConfigValue")
    }
}

// Config Implements
impl Config {
    fn default_value(prop: &str) -> ConfigValue {
        match prop {
            "default_shell" => ConfigValue::String(String::from("bash")),
            _ => panic!("Unknow error occurred at default_value fn in types.rs.")
        }
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

                self.value = ConfigValue::String(_value.unwrap().to_owned());
            },
            _ => panic!("Unknow error occurred at value_from fn in types.rs.")
        }

        Ok(())
    }
}
