//! The tab list, its menu and the stored tab groups in the config.

mod types;
mod utils;

use std::borrow::Cow;

use toml_edit::DocumentMut;

use crate::app::App;
use crate::error::{AppError, AppResult};

pub use types::{Tab, TabId, TabList};
pub use utils::{
    next,
    prev,
    quick_switch,
    switch_to_tab,
    tab_operation,
    update_current_tab,
};

pub fn read_config(app: &mut App, document: &DocumentMut) -> AppResult<()> {
    let type_err = "The type of storage_tabs in auto_config.toml is error";
    let mut errors = AppError::new();

    if let Some(item) = document.get("storage_tabs") {
        let Some(tab_groups) = item.as_array() else {
            return Err(anyhow::anyhow!("{type_err}").into())
        };

        for tab_list in tab_groups.iter() {
            // A broken group is skipped: the other groups are still stored.
            let Some(tabs) = tab_list.as_array() else {
                errors.add_error(anyhow::anyhow!("{type_err}"));
                continue
            };

            let mut stored_tabs: Vec<Cow<str>> = Vec::with_capacity(tabs.len());
            let mut valid = true;

            for tab in tabs.iter() {
                let Some(path) = tab.as_str() else {
                    valid = false;
                    break
                };

                stored_tabs.push(Cow::Owned(path.to_owned()));
            }

            if !valid {
                errors.add_error(anyhow::anyhow!(
                    "A stored tab group in auto_config.toml has a non-string path"
                ));
                continue
            }

            app.tab_list.storage.push(stored_tabs.into());
        }
    }

    if !errors.is_empty() {
        return Err(errors)
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_read_config_skips_a_broken_group() {
        let document: DocumentMut = r#"
            storage_tabs = [
                ["/tmp", "/home"],
                ["/tmp", 3],
                "/not-a-group",
            ]
        "#
            .parse()
            .expect("Invalid TOML document");

        let mut app = App::default();
        let err = read_config(&mut app, &document).unwrap_err();

        assert_eq!(app.tab_list.storage.len(), 1);
        assert_eq!(app.tab_list.storage[0].len(), 2);
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_read_config_accepts_a_missing_section() {
        let document: DocumentMut = "".parse().expect("Invalid TOML document");

        let mut app = App::default();
        assert!(read_config(&mut app, &document).is_ok());
        assert!(app.tab_list.storage.is_empty());
    }

    #[test]
    fn test_clear_storage() {
        let mut tabs = TabList::new(std::path::PathBuf::from("/tmp"));
        tabs.storage.push(
            vec![Cow::Owned(String::from("/tmp"))].into()
        );

        tabs.clear_storage();

        assert!(tabs.storage.is_empty());
    }
}
