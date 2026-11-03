// Colorscheme

use std::env::var;
use std::collections::HashMap;

use ratatui::style::{Color, Style, Modifier};

/// The palette used when `LS_COLORS` is not set (or is empty).
///
/// It is the value of `$LS_COLORS` from the author's interactive zsh (a
/// standard `dircolors` palette), so tests and fresh environments get the same
/// colours as the development machine instead of a panic or uncoloured output.
const DEFAULT_LS_COLORS: &str = include_str!("default_ls_colors.txt");

pub struct TermColors {
    pub dir_style: Style,
    pub file_style: Style,
    pub marked_style: Style,
    pub orphan_style: Style,
    pub symlink_style: Style,
    pub executable_style: Style,
}

macro_rules! hashmap {
    ($( ($key:expr, $val:expr) $(,)? )*) => {{
        let mut temp_hashmap: HashMap<&str, Style> = HashMap::new();
        {
            $(
                temp_hashmap.insert($key, $val);
            )*
        }

        temp_hashmap
    }};
}

impl TermColors {
    pub fn init() -> TermColors {
        let configured = var("LS_COLORS").ok();

        Self::from_ls_colors(configured.as_deref())
    }

    /// Build the styles from an `LS_COLORS` value.
    ///
    /// `None` and a blank value mean the variable is not usable, so the
    /// bundled palette is used. This keeps `TermColors::init` infallible: a
    /// terminal without `LS_COLORS` (CI, a bare shell, a minimal container)
    /// still gets readable colours.
    fn from_ls_colors(configured: Option<&str>) -> TermColors {
        let color_map = hashmap!(
            ("0", Style::new().fg(Color::White)),
            ("01", Style::new().add_modifier(Modifier::BOLD)),
            ("04", Style::new().add_modifier(Modifier::UNDERLINED)),
            ("07", Style::new().add_modifier(Modifier::REVERSED)),
            ("31", Style::new().fg(Color::Red)),
            ("32", Style::new().fg(Color::Green)),
            ("33", Style::new().fg(Color::LightYellow)),
            ("34", Style::new().fg(Color::Blue)),
            ("35", Style::new().fg(Color::Rgb(255, 121, 198))),
            ("36", Style::new().fg(Color::Cyan)),
            ("37", Style::new().fg(Color::Gray)),
            ("40", Style::new().bg(Color::Black)),
            ("41", Style::new().bg(Color::Red)),
            ("42", Style::new().bg(Color::Green)),
            ("43", Style::new().bg(Color::Rgb(255, 165, 0))),
            ("44", Style::new().bg(Color::Blue)),
            ("45", Style::new().bg(Color::Rgb(128, 0, 128))),
            ("46", Style::new().bg(Color::Cyan)),
            ("47", Style::new().bg(Color::Gray)),
            ("90", Style::new().fg(Color::DarkGray)),
            ("91", Style::new().fg(Color::LightRed)),
            ("92", Style::new().fg(Color::LightGreen)),
            ("93", Style::new().fg(Color::Yellow)),
            ("94", Style::new().fg(Color::LightBlue)),
            ("95", Style::new().fg(Color::Rgb(203, 195, 227))),
            ("96", Style::new().fg(Color::Rgb(64, 224, 208))),
            ("100", Style::new().bg(Color::DarkGray)),
            ("101", Style::new().bg(Color::LightRed)),
            ("102", Style::new().bg(Color::LightGreen)),
            ("103", Style::new().bg(Color::Yellow)),
            ("104", Style::new().bg(Color::LightBlue)),
            ("105", Style::new().bg(Color::Rgb(203, 195, 227))),
            ("106", Style::new().bg(Color::Rgb(64, 224, 208)))
        );

        let colors = configured
            .filter(|colors| !colors.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_LS_COLORS.trim());

        let colors: Vec<&str> = colors.split(":").collect();
        let dir_style = fetch_style(&colors, &color_map, "di");
        let file_style = fetch_style(&colors, &color_map, "rs");
        let marked_style = fetch_style(&colors, &color_map, "pi");
        let orphan_style = fetch_style(&colors, &color_map, "do");
        let symlink_style = fetch_style(&colors, &color_map, "ln");
        let executable_style = fetch_style(&colors, &color_map, "ex");

        TermColors {
            dir_style,
            file_style,
            marked_style,
            orphan_style,
            symlink_style,
            executable_style
        }
    }
}

fn fetch_style(from: &[&str],
               map: &HashMap<&str, Style>,
               target: &str
) -> Style
{
    let search_result = from
        .iter()
        .position(|color| color.starts_with(target));

    if let Some(idx) = search_result {
        let mut final_style: Option<Style> = None;
        let color_str: Vec<&str> = from[idx]
            .split("=")
            .collect::<Vec<&str>>()[1]
            .split(";")
            .collect();         // Fetch vec![0, 1] from "rs=0;1"

        for s in color_str.into_iter() {
            if let Some(get_style) = map.get(s) {
                if let Some(ref mut style) = final_style {
                    *style = style.patch(*get_style);
                } else {
                    final_style = Some(*get_style);
                }
            }
        }

        if let Some(style) = final_style {
            return style
        }
    }

    Style::default()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_missing_ls_colors_uses_the_bundled_palette() {
        let colors = TermColors::from_ls_colors(None);

        assert_eq!(
            colors.dir_style,
            Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD)
        );
        assert_eq!(colors.file_style, Style::new().fg(Color::White));
        assert_eq!(
            colors.marked_style,
            Style::new().bg(Color::Black).fg(Color::LightYellow)
        );
        assert_eq!(
            colors.orphan_style,
            Style::new()
                .fg(Color::Rgb(255, 121, 198))
                .add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            colors.symlink_style,
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            colors.executable_style,
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
        );
    }

    #[test]
    fn test_blank_ls_colors_uses_the_bundled_palette() {
        let bundled = TermColors::from_ls_colors(None);

        for value in ["", " ", "\t\n"] {
            let colors = TermColors::from_ls_colors(Some(value));

            assert_eq!(colors.dir_style, bundled.dir_style);
            assert_eq!(colors.file_style, bundled.file_style);
            assert_eq!(colors.marked_style, bundled.marked_style);
            assert_eq!(colors.orphan_style, bundled.orphan_style);
            assert_eq!(colors.symlink_style, bundled.symlink_style);
            assert_eq!(colors.executable_style, bundled.executable_style);
        }
    }

    #[test]
    fn test_present_ls_colors_is_parsed() {
        let colors = TermColors::from_ls_colors(Some(
            "di=31:rs=32:ln=33:pi=40;35:do=01;36:ex=37"
        ));

        assert_eq!(colors.dir_style, Style::new().fg(Color::Red));
        assert_eq!(colors.file_style, Style::new().fg(Color::Green));
        assert_eq!(
            colors.symlink_style,
            Style::new().fg(Color::LightYellow)
        );
        assert_eq!(
            colors.marked_style,
            Style::new().bg(Color::Black).fg(Color::Rgb(255, 121, 198))
        );
        assert_eq!(
            colors.orphan_style,
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        );
        assert_eq!(colors.executable_style, Style::new().fg(Color::Gray));
    }
}
