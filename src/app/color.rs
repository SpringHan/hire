// Colorscheme

use std::env::var;
use std::collections::HashMap;

use ratatui::style::{Color, Style, Modifier};

pub struct TermColors {
    pub dir_style: Style,
    pub file_style: Style,
    pub orphan_style: Style,
    pub symlink_style: Style,
    pub executable_style: Style
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
        let color_map = hashmap!(
            ("0", Style::new().fg(Color::White)),
            ("01", Style::new().add_modifier(Modifier::BOLD)),
            ("04", Style::new().add_modifier(Modifier::UNDERLINED)),
            ("07", Style::new().add_modifier(Modifier::REVERSED)),
            ("31", Style::new().fg(Color::Red)),
            ("32", Style::new().fg(Color::Green)),
            ("33", Style::new().fg(Color::Rgb(255, 165, 0))),
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

        let colors = var("LS_COLORS").expect("Unable to get colors");
        let colors: Vec<&str> = colors.split(":").collect();

        let dir_style = fetch_style(&colors, &color_map, "di");
        let file_style = fetch_style(&colors, &color_map, "rs");
        let orphan_style = fetch_style(&colors, &color_map, "do");
        let symlink_style = fetch_style(&colors, &color_map, "ln");
        let executable_style = fetch_style(&colors, &color_map, "ex");

        TermColors {
            dir_style,
            file_style,
            orphan_style,
            symlink_style,
            executable_style
        }
    }
}

fn fetch_style(from: &Vec<&str>,
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

        if final_style.is_some() {
            return final_style.unwrap()
        }
    }

    Style::default()
}
