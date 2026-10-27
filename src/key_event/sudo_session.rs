// Sudo session.
//
// Run a root instance of hire in the same terminal, handing over the tabs of
// the current instance. Quitting the root instance returns to this one, which
// is how the root privileges are dropped again.

use std::{
    io::{stdout, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
    fs::{self, DirBuilder, OpenOptions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
};

use anyhow::{anyhow, bail};
use toml_edit::{value, DocumentMut};
use ratatui::{
    crossterm::{
        execute,
        cursor::{Hide, Show},
        terminal::{
            enable_raw_mode,
            disable_raw_mode,
            EnterAlternateScreen,
            LeaveAlternateScreen,
        },
    },
    DefaultTerminal,
};

use crate::{app::App, config::get_conf_file, error::AppResult, rt_error};

use super::{move_cursor, tab::update_current_tab, Goto};

/// The tabs handed over from one hire instance to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTabs {
    /// The index of the tab which is focused in the new instance.
    pub current: usize,
    pub tabs: Vec<SessionTab>,
}

/// One tab of a session: its path, whether hidden files are shown and which
/// file is selected in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTab {
    pub path: PathBuf,
    pub hide_files: bool,
    pub selected: Option<usize>,
}

impl SessionTabs {
    /// Take the tabs of the running instance.
    ///
    /// The current tab is synced first, so the path and the selection of the
    /// active tab are the ones the user currently sees.
    pub fn from_app(app: &mut App) -> AppResult<Self> {
        update_current_tab(app);

        let (current, list, selected) = app.tab_list.snapshot();
        let tabs = list
            .into_iter()
            .enumerate()
            .map(|(idx, (path, hide_files))| SessionTab {
                path,
                hide_files,
                selected: selected.get(idx).copied().flatten(),
            })
            .collect::<Vec<_>>();

        Ok(Self { current, tabs })
    }

    /// Serialize the tabs into a TOML document.
    pub fn to_toml(&self) -> anyhow::Result<String> {
        let mut content = format!("current = {}\n", self.current);

        for tab in &self.tabs {
            let path = tab.path.to_str().ok_or_else(|| {
                anyhow!(
                    "The path of a tab isn't valid UTF-8: {}",
                    tab.path.display()
                )
            })?;

            content.push_str("\n[[tabs]]\n");
            content.push_str(&format!("path = {}\n", value(path)));
            content.push_str(&format!("hide_files = {}\n", tab.hide_files));

            if let Some(selected) = tab.selected {
                content.push_str(&format!("selected = {}\n", selected));
            }
        }

        Ok(content)
    }

    /// Parse the tabs from a TOML document.
    pub fn from_toml(content: &str) -> anyhow::Result<Self> {
        let document: DocumentMut = content.parse()?;

        let tables = document
            .get("tabs")
            .and_then(|item| item.as_array_of_tables())
            .ok_or_else(|| anyhow!("The session doesn't contain any `tabs`"))?;

        let mut tabs = Vec::new();
        for table in tables.iter() {
            let path = table
                .get("path")
                .and_then(|item| item.as_str())
                .ok_or_else(|| anyhow!("A tab of the session has no path"))?;

            let hide_files = table
                .get("hide_files")
                .and_then(|item| item.as_bool())
                .unwrap_or(false);

            let selected = table
                .get("selected")
                .and_then(|item| item.as_integer())
                .map(|selected| selected.max(0) as usize);

            tabs.push(SessionTab {
                path: PathBuf::from(path),
                hide_files,
                selected,
            });
        }

        if tabs.is_empty() {
            bail!("The session contains no tab")
        }

        let current = document
            .get("current")
            .and_then(|item| item.as_integer())
            .unwrap_or(0)
            .max(0) as usize;

        Ok(Self {
            current: current.min(tabs.len() - 1),
            tabs,
        })
    }

    /// Write the tabs into `path`, readable by the owner only.
    pub fn save(&self, path: &Path) -> AppResult<()> {
        let content = self.to_toml()?;

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;

        file.write_all(content.as_bytes())?;
        Ok(())
    }

    /// Read the tabs from `path`.
    pub fn load(path: &Path) -> AppResult<Self> {
        let content = fs::read_to_string(path)?;
        Ok(Self::from_toml(&content)?)
    }
}

impl<'a> App<'a> {
    /// Replace the open tabs with the ones of a previous session.
    ///
    /// The focus is placed on the tab which was focused before, together with
    /// the file selected in it.
    pub fn apply_session_tabs(&mut self, session: &SessionTabs) -> AppResult<()> {
        if session.tabs.is_empty() {
            return Ok(())
        }

        let current = session.current.min(session.tabs.len() - 1);

        let list = session.tabs
            .iter()
            .map(|tab| (tab.path.to_owned(), tab.hide_files))
            .collect::<Vec<_>>();
        let selected = session.tabs
            .iter()
            .map(|tab| tab.selected)
            .collect::<Vec<_>>();

        self.tab_list.set_snapshot(current, list, selected);

        let tab = &session.tabs[current];
        self.goto_dir(&tab.path, Some(tab.hide_files))?;

        if let Some(selected) = tab.selected.filter(|selected| *selected != 0) {
            move_cursor(self, Goto::Index(selected), self.root())?;
        }

        Ok(())
    }
}

/// Start a root instance of hire in the same terminal.
///
/// The new process is `sudo -E <hire> --sudo-session <file> --config-dir <dir>`
/// with the tabs of this instance; the root instance uses a throw-away copy of
/// the config, so it can't modify the user's own config files. Quitting it
/// returns here, which drops the root privileges.
pub fn sudo_session(app: &mut App, terminal: &mut DefaultTerminal) -> AppResult<()> {
    if is_root() {
        rt_error!("Already running as root, a sudo session isn't needed")
    }

    let session = SessionTabs::from_app(app)?;

    let session_dir = create_session_dir()?;
    let session_file = session_dir.join("session.toml");

    let result = prepare_session(&session, &session_dir, &session_file)
        .and_then(|_| {
            launch(app, terminal, &session_dir, &session_file)
        });

    // The directory holds the tab state and the throw-away config; it's no
    // longer needed once the sudo session finished.
    let _ = fs::remove_dir_all(&session_dir);

    result
}

/// Whether this process already runs as root.
fn is_root() -> bool {
    // SAFETY: `geteuid` has no preconditions and never fails.
    unsafe { libc::geteuid() == 0 }
}

/// Create a private directory for the session state and the config copy.
fn create_session_dir() -> AppResult<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    let dir = std::env::temp_dir().join(
        format!("hire-sudo-{}-{}", std::process::id(), nanos)
    );

    DirBuilder::new().mode(0o700).create(&dir)?;
    Ok(dir)
}

/// Write the tab state and copy the config into the session directory.
fn prepare_session(
    session: &SessionTabs,
    session_dir: &Path,
    session_file: &Path
) -> AppResult<()> {
    session.save(session_file)?;
    copy_config_to(session_dir)?;
    Ok(())
}

/// Copy the config files, so that the sudo session has the same keymap and
/// settings, while its own changes are discarded when the session is over.
fn copy_config_to(dir: &Path) -> AppResult<()> {
    let (auto_config, user_config, keymap) = get_conf_file()?;

    for path in [auto_config, user_config, keymap] {
        let source = Path::new(&path);
        if !source.is_file() {
            continue
        }

        let file_name = source.file_name().ok_or_else(|| {
            anyhow!("Invalid config file path: {}", source.display())
        })?;

        fs::copy(source, dir.join(file_name))?;
    }

    Ok(())
}

/// Run the sudo session and refresh the file lists afterwards.
fn launch(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    session_dir: &Path,
    session_file: &Path
) -> AppResult<()> {
    let exe = std::env::current_exe()?;

    // Remember the selected file, so the focus can be restored after the
    // file lists are refreshed.
    let current_file = app.get_file_saver()
        .map(|file| file.name.to_owned());

    let mut command = Command::new("sudo");
    command
        .arg("-E")
        .arg(exe)
        .arg("--sudo-session")
        .arg(session_file)
        .arg("--config-dir")
        .arg(session_dir);

    let status = run_in_terminal(terminal, &mut command)?;

    if !status.success() {
        rt_error!(format!("The sudo session exited with {status}"))
    }

    // The root session may have changed the current directory.
    app.goto_dir(app.current_path(), Some(app.hide_files))?;
    if let Some(name) = current_file {
        app.file_search_sync(name, true)?;
    }

    Ok(())
}

/// Run `command` on the normal screen and always restore the TUI afterwards.
fn run_in_terminal(
    terminal: &mut DefaultTerminal,
    command: &mut Command
) -> AppResult<ExitStatus> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen, Show)?;

    let status = command.status();

    // `sudo` asks for the password on the normal screen; restore the TUI even
    // when it couldn't be started at all.
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, Hide)?;
    terminal.clear()?;

    Ok(status?)
}

#[cfg(test)]
mod test {
    use super::*;

    fn tab(path: &str, hide_files: bool, selected: Option<usize>) -> SessionTab {
        SessionTab {
            path: PathBuf::from(path),
            hide_files,
            selected,
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-session-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_session_tabs_toml_roundtrip() {
        let session = SessionTabs {
            current: 1,
            tabs: vec![
                tab("/home/spring", true, None),
                tab("/tmp/a b/中文'\"x", false, Some(3)),
            ],
        };

        let content = session.to_toml().unwrap();
        assert_eq!(SessionTabs::from_toml(&content).unwrap(), session);
    }

    #[test]
    fn test_session_tabs_save_and_load() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("save");
        let path = dir.join("session.toml");

        let session = SessionTabs {
            current: 0,
            tabs: vec![tab("/tmp", false, Some(1))],
        };

        session.save(&path).unwrap();
        assert_eq!(SessionTabs::load(&path).unwrap(), session);

        // The state file is readable by its owner only.
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_session_tabs_clamps_current() {
        let content = "current = 9\n\n[[tabs]]\npath = \"/tmp\"\n";
        let session = SessionTabs::from_toml(content).unwrap();

        assert_eq!(session.current, 0);
        assert_eq!(session.tabs.len(), 1);
        assert!(!session.tabs[0].hide_files);
    }

    #[test]
    fn test_session_tabs_requires_tabs() {
        assert!(SessionTabs::from_toml("current = 0\n").is_err());
        assert!(SessionTabs::from_toml("current = 0\ntabs = []\n").is_err());
    }

    #[test]
    fn test_apply_session_tabs() {
        let base = temp_dir("apply");
        let first = base.join("first");
        let second = base.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        for name in ["a", "b", "c"] {
            fs::write(second.join(name), "").unwrap();
        }

        let session = SessionTabs {
            current: 1,
            tabs: vec![
                tab(first.to_str().unwrap(), true, None),
                tab(second.to_str().unwrap(), false, Some(2)),
            ],
        };

        let mut app = App::default();
        app.apply_session_tabs(&session).unwrap();

        assert_eq!(app.path, second);
        assert!(!app.hide_files);
        assert_eq!(app.tab_list.len(), 2);
        assert_eq!(app.tab_list.current(), 1);
        assert_eq!(app.selected_item.current_selected(), Some(2));

        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_session_tabs_from_app() {
        let base = temp_dir("from-app");
        let first = base.join("first");
        let second = base.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("file"), "").unwrap();

        let mut app = App::default();
        app.tab_list.set_snapshot(
            1,
            vec![(first.to_owned(), true), (second.to_owned(), false)],
            vec![None, None]
        );
        app.goto_dir(&second, Some(false)).unwrap();

        let session = SessionTabs::from_app(&mut app).unwrap();

        assert_eq!(session.current, 1);
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.tabs[0].path, first);
        assert!(session.tabs[0].hide_files);
        assert_eq!(session.tabs[1].path, second);
        assert_eq!(session.tabs[1].selected, Some(0));

        fs::remove_dir_all(&base).unwrap();
    }
}
