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
    fn default_value(prop: &str) -> ConfigValue<'_> {
        match prop {
            "gui_commands" => ConfigValue::Vec(Vec::new()),
            "default_shell" => ConfigValue::String(Cow::Borrowed("bash")),
            "file_read_program" => ConfigValue::String(Cow::Borrowed("vim")),
            "gui_file_open_cmd" => ConfigValue::String(Cow::Borrowed("")),
            "shell_prefix" => ConfigValue::String(Cow::Borrowed("C-s")),
            _ => panic!("Unknow error occurred at default_value fn in types.rs.")
        }
    }

    pub fn find_value<'b>(
        configs: &'b Vec<Self>,
        prop: &str
    ) -> Option<&'b ConfigValue<'a>>
    {
        configs.iter()
            .find(|conf| conf.name == prop)
            .map(|conf| &conf.value)
    }

    pub fn get_value(configs: &'a Vec<Self>, prop: &str) -> &'a ConfigValue<'a> {
        match Self::find_value(configs, prop) {
            Some(value) => value,
            None => panic!("Error in code at get_value fn in config/types.rs!"),
        }
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
            "default_shell" | "file_read_program" | "gui_file_open_cmd" |
            "shell_prefix" =>
                self.value = Self::get_str(value, err_msg)?,

            "gui_commands" => {
                let Some(items) = value.as_array() else {
                    bail!("{err_msg}")
                };

                // The list is built first and stored last: a broken entry skips
                // the whole property (it keeps its default) instead of leaving
                // the commands read before it in place.
                let mut commands = Vec::with_capacity(items.len());

                for item in items.iter() {
                    let Some(command) = item.as_str() else {
                        bail!("Meet type error when setting gui_commands")
                    };

                    commands.push(Cow::Owned(command.to_owned()));
                }

                self.value = ConfigValue::Vec(commands);
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
