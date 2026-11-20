// Syntax highlighting of the file content preview.

use std::env;
use std::path::Path;

use lazy_static::lazy_static;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SynStyle, Theme, ThemeSet};
use syntect::parsing::{
    SyntaxDefinition, SyntaxReference, SyntaxSet, SyntaxSetBuilder
};
use syntect::util::LinesWithEndings;

/// Syntax definitions syntect's bundled set does not ship.
///
/// The bundled set is Sublime's default package, which has no TOML and no
/// INI-style syntax, so `Cargo.toml`, `Cargo.lock` and `.conf` files would be
/// shown as plain text. These two definitions cover them; they are compiled
/// into the binary.
const EXTRA_SYNTAXES: [&str; 2] = [
    include_str!("syntaxes/toml.sublime-syntax"),
    include_str!("syntaxes/ini.sublime-syntax"),
];

lazy_static! {
    /// syntect's bundled syntax definitions, loaded once.
    static ref SYNTAX_SET: SyntaxSet = SyntaxSet::load_defaults_newlines();

    /// The definitions of `EXTRA_SYNTAXES`, in a set of their own.
    ///
    /// They are not merged into `SYNTAX_SET`: that needs `into_builder()` and
    /// `build()`, which re-link all 75 bundled syntaxes and cost hundreds of
    /// milliseconds. A path is looked up in `SYNTAX_SET` first and only a file
    /// that set does not know is looked up here, so two sets cost nothing.
    static ref EXTRA_SET: SyntaxSet = {
        let mut set = SyntaxSetBuilder::new();

        for source in EXTRA_SYNTAXES {
            set.add(
                SyntaxDefinition::load_from_str(source, true, None)
                    .expect("the bundled syntax definition is valid"),
            );
        }

        set.build()
    };

    /// The theme the preview is highlighted with, see `theme_name`.
    static ref THEME: Theme = {
        let mut themes = ThemeSet::load_defaults().themes;

        themes
            .remove(theme_name())
            .expect("the theme is bundled with syntect")
    };
}

/// Highlight `content`, the preview of the file at `path`.
///
/// `None` when there is nothing to highlight: no set knows a language for the
/// file (a `.txt` file included), the content is too large to be worth it, or
/// the parser fails. The caller keeps the unhighlighted text it is showing.
///
/// This runs on the highlighting thread (`app/highlight.rs`), never on the UI
/// thread: syntect parses every line of the window for every cursor movement.
pub fn highlight_text(content: &str, path: &Path) -> Option<Text<'static>> {
    let (set, syntax) = syntax_for(path)?;

    highlighted_text(content, set, syntax)
}

/// The set and the syntax to highlight `path` with.
///
/// `None` when neither set knows a language for the file, and for syntect's
/// plain text syntax (a `.txt` file is shown exactly as before). The detection
/// is syntect's own: extension, then file name, then the first line of the file
/// (so it can open the file — another reason to run on the worker thread).
fn syntax_for(
    path: &Path
) -> Option<(&'static SyntaxSet, &'static SyntaxReference)> {
    let mut found = None;

    for set in [&*SYNTAX_SET, &*EXTRA_SET] {
        if let Some(syntax) = set.find_syntax_for_file(path).ok().flatten() {
            found = Some((set, syntax));
            break
        }
    }

    let (set, syntax) = found?;

    if syntax.name == "Plain Text" {
        return None
    }

    Some((set, syntax))
}

/// How much of a preview is still worth highlighting.
///
/// The read stops after one window of lines, so only a file with extremely long
/// lines reaches this (a minified JSON, a base64 blob). syntect parses every
/// byte of such a line while the terminal can only show its first columns, and
/// the highlight runs again for every cursor movement, so such a file is shown
/// unhighlighted instead.
const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;

/// Highlight `content` with `syntax`, which belongs to `set`.
///
/// `None` when the content is too large to be worth highlighting, or when
/// syntect fails on a line (a syntax definition with a broken regular
/// expression); the caller then shows the unhighlighted text instead.
fn highlighted_text(
    content: &str,
    set: &SyntaxSet,
    syntax: &SyntaxReference,
) -> Option<Text<'static>>
{
    if content.len() > MAX_HIGHLIGHT_BYTES {
        return None
    }

    let mut highlighter = HighlightLines::new(syntax, &THEME);
    let mut text = Text::default();

    // `LinesWithEndings` keeps the newline: the parser needs it to know where
    // a line ends (a block comment does not end with the line).
    for line in LinesWithEndings::from(content) {
        let ranges = highlighter.highlight_line(line, set).ok()?;

        let spans: Vec<Span> = ranges
            .into_iter()
            .map(|(style, piece)| {
                // The line break is drawn by the `Line`, not by a span.
                let piece = piece.strip_suffix('\n').unwrap_or(piece);

                // A tab has no width in a `Span`.
                Span::styled(piece.replace('\t', "    "), convert(style))
            })
            .filter(|span| !span.content.is_empty())
            .collect();

        text.lines.push(Line::from(spans));
    }

    Some(text)
}

/// Map a syntect style onto the colours and attributes ratatui draws.
///
/// Only the foreground is used: the theme's background belongs to an editor
/// window, the preview is drawn on the terminal's own background.
fn convert(style: SynStyle) -> Style {
    let foreground = style.foreground;
    let mut converted =
        Style::new().fg(Color::Rgb(foreground.r, foreground.g, foreground.b));

    if style.font_style.contains(FontStyle::BOLD) {
        converted = converted.add_modifier(Modifier::BOLD);
    }

    if style.font_style.contains(FontStyle::ITALIC) {
        converted = converted.add_modifier(Modifier::ITALIC);
    }

    if style.font_style.contains(FontStyle::UNDERLINE) {
        converted = converted.add_modifier(Modifier::UNDERLINED);
    }

    converted
}

/// The name of the theme to highlight with.
///
/// hire does not know the terminal's background, so `$COLORFGBG` (`"fg;bg"`,
/// set by many terminals) picks between the light and the dark palette; a
/// terminal which does not export it gets the dark one. Both themes are
/// bundled with syntect.
fn theme_name() -> &'static str {
    if light_background() {
        "base16-ocean.light"
    } else {
        "base16-ocean.dark"
    }
}

/// Whether `$COLORFGBG` says that the terminal background is light.
fn light_background() -> bool {
    env::var("COLORFGBG").is_ok_and(|value| is_light_background(&value))
}

/// Whether a `$COLORFGBG` value (`"fg;bg"`) names a light background.
///
/// The background is the last field (`"15;0"` is white text on a black
/// background); palette entries 0-6 are the dark colours and 7 and above the
/// light ones.
fn is_light_background(value: &str) -> bool {
    let background = value.rsplit(';').next().unwrap_or("");

    background.trim().parse::<u8>().is_ok_and(|index| index >= 7)
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::utils::{plain_text, read_text_prefix};

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let dir = std::env::temp_dir().join(format!(
            "hire-syntax-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));

        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &Path, content: &[u8]) -> fs::File {
        let mut file = fs::File::create(path).unwrap();
        file.write_all(content).unwrap();

        fs::File::open(path).unwrap()
    }

    /// What the preview shows for `path`: the plain text, then the highlighted
    /// text when there is a language to highlight.
    fn preview(path: &Path, content: &[u8]) -> Text<'static> {
        let file = write_file(path, content);
        let content = read_text_prefix(&file).unwrap();

        match highlight_text(&content, path) {
            Some(text) => text,
            None => plain_text(&content).unwrap(),
        }
    }

    fn styles(text: &Text<'_>) -> Vec<Style> {
        text.lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.style)
            .collect()
    }

    fn content(text: &Text<'_>) -> String {
        text.lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    /// A source file is highlighted: the keywords and the code get different
    /// foreground colours from the theme.
    #[test]
    fn test_source_file_is_highlighted() {
        let dir = temp_dir("source");
        let path = dir.join("main.rs");

        let text = preview(
            &path,
            b"// hi\nfn main() {\n    let answer = 42;\n}\n"
        );

        assert_eq!(text.lines.len(), 4);
        assert_eq!(
            content(&text),
            "// hi\nfn main() {\n    let answer = 42;\n}"
        );

        let styles = styles(&text);
        assert!(styles.len() > 1);

        let first = styles[0];
        assert!(
            styles.iter().any(|style| *style != first),
            "the theme colours more than the default foreground"
        );

        // Every span has the theme's foreground instead of the terminal
        // default, which is what makes the preview coloured.
        assert!(
            styles.iter().all(|style| style.fg.is_some()),
            "{styles:?}"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A tab is expanded like in the plain preview, and highlighting does not
    /// leave a tab in a span.
    #[test]
    fn test_tabs_are_expanded() {
        let dir = temp_dir("tab");
        let path = dir.join("main.rs");

        let text = preview(&path, b"fn\tmain() {}\n");

        assert!(!content(&text).contains('\t'), "{}", content(&text));
        assert_eq!(content(&text), "fn    main() {}");

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A `.txt` file is syntect's plain text syntax: the preview shows it as
    /// before, i.e. every span keeps the terminal's default style.
    #[test]
    fn test_plain_text_is_not_highlighted() {
        let dir = temp_dir("plain");
        let path = dir.join("notes.txt");

        let text = preview(&path, b"fn main() {\n}\n");

        let styles = styles(&text);
        assert!(!styles.is_empty());
        assert!(
            styles.iter().all(|style| *style == styles[0]),
            "plain text keeps one style"
        );
        assert!(styles[0].fg.is_none(), "{:?}", styles[0]);

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A file whose lines are far too long for the screen is not handed to
    /// syntect: it would parse every byte of a line the terminal only shows the
    /// first columns of, on every cursor movement.
    #[test]
    fn test_huge_line_is_not_highlighted() {
        let dir = temp_dir("huge");
        let path = dir.join("minified.json");

        let line = "let x = 1; ".repeat(300 * 1024 / 11 + 1);
        let text = preview(&path, line.as_bytes());

        let styles = styles(&text);
        assert!(styles.iter().all(|style| *style == styles[0]));
        assert!(styles[0].fg.is_none(), "{:?}", styles[0]);

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A file without an extension or a known one is not highlighted either,
    /// so the fallback does not lose the ANSI colours a log may contain.
    #[test]
    fn test_unknown_file_keeps_ansi_colours() {
        let dir = temp_dir("ansi");
        let path = dir.join("run.log");

        let text = preview(&path, b"\x1b[31mred\x1b[0m plain\n");

        let styles = styles(&text);
        assert!(
            styles.contains(&Style::new().fg(Color::Red)),
            "{styles:?}"
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    /// A file which is not valid UTF-8 is an error, which is how the preview
    /// decides that the file is not text at all (image or MIME type instead).
    #[test]
    fn test_binary_file_is_an_error() {
        let dir = temp_dir("binary");
        let path = dir.join("image.png");

        let file = write_file(&path, &[0xff, 0xfe, 0x00, 0x01]);

        assert!(read_text_prefix(&file).is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    /// The background of the terminal is guessed from `$COLORFGBG`; only the
    /// last field (the background palette index) matters.
    #[test]
    fn test_light_background_guess() {
        assert!(!is_light_background("15;0"));
        assert!(!is_light_background("0"));
        assert!(!is_light_background(""));
        assert!(!is_light_background("0;6"));
        assert!(is_light_background("0;7"));
        assert!(is_light_background("0;15"));
        assert!(is_light_background("0; 15"));
    }

    /// Both themes `theme_name` can return are bundled with syntect, which is
    /// what the `expect` in the `THEME` initializer relies on.
    #[test]
    fn test_themes_are_bundled() {
        let themes = ThemeSet::load_defaults().themes;

        assert!(themes.contains_key("base16-ocean.dark"));
        assert!(themes.contains_key("base16-ocean.light"));
    }

    /// `EXTRA_SYNTAXES` is what makes configuration files highlighted: they are
    /// the formats syntect's bundled set does not know, so these extensions had
    /// no syntax and the files were shown as plain text.
    #[test]
    fn test_config_files_are_highlighted() {
        let dir = temp_dir("config");
        let path = dir.join("Cargo.toml");

        let text = preview(
            &path,
            b"# comment\n[package]\nname = \"hire\"\ncount = 42\n"
        );

        assert_eq!(
            content(&text),
            "# comment\n[package]\nname = \"hire\"\ncount = 42"
        );

        let colours = styles(&text);
        assert!(
            colours.iter().any(|style| *style != colours[0]),
            "the TOML file is coloured"
        );

        // The comment, the table, the key and the value each get their own
        // colour, so the preview is more than one style of highlight.
        let distinct = colours
            .iter()
            .filter(|style| **style != colours[0])
            .count();
        assert!(distinct >= 2, "{colours:?}");

        // `.lock` is TOML as well; `.conf`, `.ini` and `.cfg` are INI-shaped.
        for name in ["Cargo.lock", "app.conf", "app.ini", "app.cfg"] {
            let path = dir.join(name);
            let text = preview(&path, b"[section]\nkey = value\n");
            let colours = styles(&text);

            assert!(
                colours.iter().any(|style| *style != colours[0]),
                "{name} is coloured"
            );
        }

        fs::remove_dir_all(&dir).unwrap();
    }

    /// The extensions and file names the extra syntaxes claim. A file which
    /// neither syntect nor the extra definitions know stays plain.
    #[test]
    fn test_config_extensions_have_a_syntax() {
        for name in [
            "Cargo.toml",
            "Cargo.lock",
            "app.conf",
            "app.ini",
            "app.cfg",
            "app.properties",
            "app.desktop",
            ".editorconfig",
            ".env",
        ] {
            assert!(
                syntax_for(Path::new(name)).is_some(),
                "{name} should have a syntax"
            );
        }

        assert!(syntax_for(Path::new("notes.unknown")).is_none());
    }
}
