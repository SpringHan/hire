// Tab.

mod types;
mod utils;

use std::borrow::Cow;

use toml_edit::DocumentMut;

use crate::{app::App, option_get};

pub use types::TabList;
pub use utils::{tab_operation, quick_switch, prev, next};

pub fn read_config(app: &mut App, document: &DocumentMut) -> anyhow::Result<()> {
    if let Some(item) = document.get("storage_tabs") {
        let type_err = "The type of storage_tabs in auto_config.toml is error";

        for tab_list in option_get!(item.as_array(), type_err).iter() {
            let mut stored_tabs: Vec<Cow<str>> = Vec::new();
            for tab in option_get!(tab_list.as_array(), type_err).iter() {
                stored_tabs.push(Cow::Owned(
                    option_get!(tab.as_str(), type_err).to_owned()
                ));
            }

            app.tab_list.storage.push(stored_tabs.into());
        }
    }

    Ok(())
}
