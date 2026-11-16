//! Theme palettes and the current render palette.

use std::cell::Cell;

use ratatui::style::Color;
use tsuiyomi_core::Theme;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub base: Color,
    pub surface0: Color,
    pub surface1: Color,
    pub overlay0: Color,
    pub overlay1: Color,
    pub subtext1: Color,
    pub text: Color,
    pub accent: Color,
    pub blue: Color,
    pub teal: Color,
    pub green: Color,
    pub yellow: Color,
    pub peach: Color,
    pub mauve: Color,
}

pub const THEMES: [Theme; 13] = [
    Theme::CatppuccinMocha,
    Theme::CatppuccinMacchiato,
    Theme::CatppuccinFrappe,
    Theme::CatppuccinLatte,
    Theme::Nord,
    Theme::SolarizedDark,
    Theme::SolarizedLight,
    Theme::GruvboxDark,
    Theme::GruvboxLight,
    Theme::MaterialDark,
    Theme::Dracula,
    Theme::OneDark,
    Theme::TokyoNight,
];

impl Palette {
    fn catppuccin_mocha() -> Self {
        Self {
            base: rgb(0x1e1e2e),
            surface0: rgb(0x313244),
            surface1: rgb(0x45475a),
            overlay0: rgb(0x6c7086),
            overlay1: rgb(0x7f849c),
            subtext1: rgb(0xbac2de),
            text: rgb(0xcdd6f4),
            accent: rgb(0xb4befe),
            blue: rgb(0x89b4fa),
            teal: rgb(0x94e2d5),
            green: rgb(0xa6e3a1),
            yellow: rgb(0xf9e2af),
            peach: rgb(0xfab387),
            mauve: rgb(0xcba6f7),
        }
    }

    fn catppuccin_macchiato() -> Self {
        Self {
            base: rgb(0x24273a),
            surface0: rgb(0x363a4f),
            surface1: rgb(0x494d64),
            overlay0: rgb(0x6e738d),
            overlay1: rgb(0x8087a2),
            subtext1: rgb(0xb8c0e0),
            text: rgb(0xcad3f5),
            accent: rgb(0xb7bdf8),
            blue: rgb(0x8aadf4),
            teal: rgb(0x8bd5ca),
            green: rgb(0xa6da95),
            yellow: rgb(0xeed49f),
            peach: rgb(0xf5a97f),
            mauve: rgb(0xc6a0f6),
        }
    }

    fn catppuccin_frappe() -> Self {
        Self {
            base: rgb(0x303446),
            surface0: rgb(0x414559),
            surface1: rgb(0x51576d),
            overlay0: rgb(0x737994),
            overlay1: rgb(0x838ba7),
            subtext1: rgb(0xb5bfe2),
            text: rgb(0xc6d0f5),
            accent: rgb(0xbabbf1),
            blue: rgb(0x8caaee),
            teal: rgb(0x81c8be),
            green: rgb(0xa6d189),
            yellow: rgb(0xe5c890),
            peach: rgb(0xef9f76),
            mauve: rgb(0xca9ee6),
        }
    }

    fn catppuccin_latte() -> Self {
        Self {
            base: rgb(0xeff1f5),
            surface0: rgb(0xccd0da),
            surface1: rgb(0xbcc0cc),
            overlay0: rgb(0x9ca0b0),
            overlay1: rgb(0x8c8fa1),
            subtext1: rgb(0x5c5f77),
            text: rgb(0x4c4f69),
            accent: rgb(0x7287fd),
            blue: rgb(0x1e66f5),
            teal: rgb(0x179299),
            green: rgb(0x40a02b),
            yellow: rgb(0xdf8e1d),
            peach: rgb(0xfe640b),
            mauve: rgb(0x8839ef),
        }
    }

    fn nord() -> Self {
        Self {
            base: rgb(0x2e3440),
            surface0: rgb(0x3b4252),
            surface1: rgb(0x434c5e),
            overlay0: rgb(0x4c566a),
            overlay1: rgb(0x616e88),
            subtext1: rgb(0xd8dee9),
            text: rgb(0xeceff4),
            accent: rgb(0x88c0d0),
            blue: rgb(0x81a1c1),
            teal: rgb(0x8fbcbb),
            green: rgb(0xa3be8c),
            yellow: rgb(0xebcb8b),
            peach: rgb(0xd08770),
            mauve: rgb(0xb48ead),
        }
    }

    fn solarized_dark() -> Self {
        Self {
            base: rgb(0x002b36),
            surface0: rgb(0x073642),
            surface1: rgb(0x0f4b56),
            overlay0: rgb(0x586e75),
            overlay1: rgb(0x657b83),
            subtext1: rgb(0x93a1a1),
            text: rgb(0xeee8d5),
            accent: rgb(0x268bd2),
            blue: rgb(0x268bd2),
            teal: rgb(0x2aa198),
            green: rgb(0x859900),
            yellow: rgb(0xb58900),
            peach: rgb(0xcb4b16),
            mauve: rgb(0x6c71c4),
        }
    }

    fn solarized_light() -> Self {
        Self {
            base: rgb(0xfdf6e3),
            surface0: rgb(0xeee8d5),
            surface1: rgb(0xe4ddc8),
            overlay0: rgb(0x93a1a1),
            overlay1: rgb(0x839496),
            subtext1: rgb(0x586e75),
            text: rgb(0x073642),
            accent: rgb(0x268bd2),
            blue: rgb(0x268bd2),
            teal: rgb(0x2aa198),
            green: rgb(0x859900),
            yellow: rgb(0xb58900),
            peach: rgb(0xcb4b16),
            mauve: rgb(0x6c71c4),
        }
    }

    fn gruvbox_dark() -> Self {
        Self {
            base: rgb(0x282828),
            surface0: rgb(0x3c3836),
            surface1: rgb(0x504945),
            overlay0: rgb(0x665c54),
            overlay1: rgb(0x7c6f64),
            subtext1: rgb(0xd5c4a1),
            text: rgb(0xebdbb2),
            accent: rgb(0xd3869b),
            blue: rgb(0x83a598),
            teal: rgb(0x8ec07c),
            green: rgb(0xb8bb26),
            yellow: rgb(0xfabd2f),
            peach: rgb(0xfe8019),
            mauve: rgb(0xd3869b),
        }
    }

    fn gruvbox_light() -> Self {
        Self {
            base: rgb(0xfbf1c7),
            surface0: rgb(0xebdbb2),
            surface1: rgb(0xd5c4a1),
            overlay0: rgb(0xa89984),
            overlay1: rgb(0x928374),
            subtext1: rgb(0x504945),
            text: rgb(0x3c3836),
            accent: rgb(0x8f3f71),
            blue: rgb(0x458588),
            teal: rgb(0x689d6a),
            green: rgb(0x79740e),
            yellow: rgb(0xb57614),
            peach: rgb(0xaf3a03),
            mauve: rgb(0x8f3f71),
        }
    }

    fn material_dark() -> Self {
        Self {
            base: rgb(0x263238),
            surface0: rgb(0x2e3c43),
            surface1: rgb(0x37474f),
            overlay0: rgb(0x546e7a),
            overlay1: rgb(0x607d8b),
            subtext1: rgb(0xb0bec5),
            text: rgb(0xeceff1),
            accent: rgb(0x82aaff),
            blue: rgb(0x82aaff),
            teal: rgb(0x89ddff),
            green: rgb(0xc3e88d),
            yellow: rgb(0xffcb6b),
            peach: rgb(0xf78c6c),
            mauve: rgb(0xc792ea),
        }
    }

    fn dracula() -> Self {
        Self {
            base: rgb(0x282a36),
            surface0: rgb(0x44475a),
            surface1: rgb(0x4d5066),
            overlay0: rgb(0x6272a4),
            overlay1: rgb(0x7a86b8),
            subtext1: rgb(0xbfc7e0),
            text: rgb(0xf8f8f2),
            accent: rgb(0xbd93f9),
            blue: rgb(0x8be9fd),
            teal: rgb(0x8be9fd),
            green: rgb(0x50fa7b),
            yellow: rgb(0xf1fa8c),
            peach: rgb(0xffb86c),
            mauve: rgb(0xff79c6),
        }
    }

    fn one_dark() -> Self {
        Self {
            base: rgb(0x282c34),
            surface0: rgb(0x31353f),
            surface1: rgb(0x3a3f4b),
            overlay0: rgb(0x5c6370),
            overlay1: rgb(0x6b7280),
            subtext1: rgb(0xabb2bf),
            text: rgb(0xdcdfe4),
            accent: rgb(0x61afef),
            blue: rgb(0x61afef),
            teal: rgb(0x56b6c2),
            green: rgb(0x98c379),
            yellow: rgb(0xe5c07b),
            peach: rgb(0xd19a66),
            mauve: rgb(0xc678dd),
        }
    }

    fn tokyo_night() -> Self {
        Self {
            base: rgb(0x1a1b26),
            surface0: rgb(0x24283b),
            surface1: rgb(0x2f3549),
            overlay0: rgb(0x565f89),
            overlay1: rgb(0x737aa2),
            subtext1: rgb(0xa9b1d6),
            text: rgb(0xc0caf5),
            accent: rgb(0x7aa2f7),
            blue: rgb(0x7aa2f7),
            teal: rgb(0x7dcfff),
            green: rgb(0x9ece6a),
            yellow: rgb(0xe0af68),
            peach: rgb(0xff9e64),
            mauve: rgb(0xbb9af7),
        }
    }
}

pub fn palette_for(theme: Theme) -> Palette {
    match theme {
        Theme::CatppuccinMocha | Theme::Unknown => Palette::catppuccin_mocha(),
        Theme::CatppuccinMacchiato => Palette::catppuccin_macchiato(),
        Theme::CatppuccinFrappe => Palette::catppuccin_frappe(),
        Theme::CatppuccinLatte => Palette::catppuccin_latte(),
        Theme::Nord => Palette::nord(),
        Theme::SolarizedDark => Palette::solarized_dark(),
        Theme::SolarizedLight => Palette::solarized_light(),
        Theme::GruvboxDark => Palette::gruvbox_dark(),
        Theme::GruvboxLight => Palette::gruvbox_light(),
        Theme::MaterialDark => Palette::material_dark(),
        Theme::Dracula => Palette::dracula(),
        Theme::OneDark => Palette::one_dark(),
        Theme::TokyoNight => Palette::tokyo_night(),
    }
}

pub fn theme_label(theme: Theme) -> &'static str {
    match theme {
        Theme::CatppuccinMocha | Theme::Unknown => "Catppuccin Mocha",
        Theme::CatppuccinMacchiato => "Catppuccin Macchiato",
        Theme::CatppuccinFrappe => "Catppuccin Frappe",
        Theme::CatppuccinLatte => "Catppuccin Latte",
        Theme::Nord => "Nord",
        Theme::SolarizedDark => "Solarized Dark",
        Theme::SolarizedLight => "Solarized Light",
        Theme::GruvboxDark => "Gruvbox Dark",
        Theme::GruvboxLight => "Gruvbox Light",
        Theme::MaterialDark => "Material Dark",
        Theme::Dracula => "Dracula",
        Theme::OneDark => "One Dark",
        Theme::TokyoNight => "Tokyo Night",
    }
}

/// Named accent choices from the current theme palette.
pub fn accent_choices(palette: &Palette) -> Vec<(&'static str, Color)> {
    vec![
        ("default", palette.accent),
        ("blue", palette.blue),
        ("teal", palette.teal),
        ("green", palette.green),
        ("yellow", palette.yellow),
        ("peach", palette.peach),
        ("mauve", palette.mauve),
    ]
}

pub fn resolve_accent(palette: &Palette, value: &str) -> Option<Color> {
    let value = value.trim();
    if let Some(color) = parse_hex(value) {
        return Some(color);
    }
    accent_choices(palette)
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(value))
        .map(|(_, color)| color)
}

pub fn parse_hex(value: &str) -> Option<Color> {
    let value = value.trim();
    let value = value.strip_prefix('#').unwrap_or(value);
    let (r, g, b) = match value.len() {
        3 => (
            u8::from_str_radix(&value[0..1], 16).ok()? * 17,
            u8::from_str_radix(&value[1..2], 16).ok()? * 17,
            u8::from_str_radix(&value[2..3], 16).ok()? * 17,
        ),
        6 => (
            u8::from_str_radix(&value[0..2], 16).ok()?,
            u8::from_str_radix(&value[2..4], 16).ok()?,
            u8::from_str_radix(&value[4..6], 16).ok()?,
        ),
        _ => return None,
    };
    Some(Color::Rgb(r, g, b))
}

thread_local! {
    static CURRENT: Cell<Palette> = Cell::new(Palette::catppuccin_mocha());
}

pub fn apply(theme: Theme, accent: Option<&str>) {
    let base = palette_for(theme);
    let accent = accent.and_then(|value| resolve_accent(&base, value));
    let mut palette = base;
    if let Some(accent) = accent {
        palette.accent = accent;
    }
    CURRENT.with(|current| current.set(palette));
}

pub fn current() -> Palette {
    CURRENT.with(Cell::get)
}

fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn c_base() -> Color {
    current().base
}
pub fn c_surface0() -> Color {
    current().surface0
}
pub fn c_surface1() -> Color {
    current().surface1
}
pub fn c_overlay0() -> Color {
    current().overlay0
}
pub fn c_overlay1() -> Color {
    current().overlay1
}
pub fn c_subtext1() -> Color {
    current().subtext1
}
pub fn c_text() -> Color {
    current().text
}
pub fn c_accent() -> Color {
    current().accent
}
pub fn c_blue() -> Color {
    current().blue
}
pub fn c_teal() -> Color {
    current().teal
}
pub fn c_green() -> Color {
    current().green
}
pub fn c_yellow() -> Color {
    current().yellow
}
pub fn c_peach() -> Color {
    current().peach
}
pub fn c_mauve() -> Color {
    current().mauve
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_and_long_hex() {
        assert_eq!(parse_hex("#abc"), Some(Color::Rgb(0xaa, 0xbb, 0xcc)));
        assert_eq!(parse_hex("aabbcc"), Some(Color::Rgb(0xaa, 0xbb, 0xcc)));
        assert_eq!(parse_hex("#AABBCC"), Some(Color::Rgb(0xaa, 0xbb, 0xcc)));
        assert_eq!(parse_hex("#abcd"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
        assert_eq!(parse_hex("blue"), None);
    }

    #[test]
    fn resolves_named_and_hex_accents() {
        let base = palette_for(Theme::Nord);
        assert_eq!(resolve_accent(&base, "blue"), Some(base.blue));
        assert_eq!(resolve_accent(&base, "BLUE"), Some(base.blue));
        assert_eq!(
            resolve_accent(&base, "#ff0000"),
            Some(Color::Rgb(255, 0, 0))
        );
        assert_eq!(resolve_accent(&base, "ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(resolve_accent(&base, "not-a-color"), None);
    }

    #[test]
    fn applying_a_theme_and_accent_updates_the_current_palette() {
        apply(Theme::GruvboxDark, None);
        let base = palette_for(Theme::GruvboxDark);
        assert_eq!(current().base, base.base);
        assert_eq!(current().accent, base.accent);

        apply(Theme::GruvboxDark, Some("teal"));
        assert_eq!(current().accent, base.teal);

        apply(Theme::GruvboxDark, Some("#123456"));
        assert_eq!(current().accent, Color::Rgb(0x12, 0x34, 0x56));
    }

    #[test]
    fn every_builtin_theme_has_a_label() {
        for theme in THEMES {
            assert!(!theme_label(theme).is_empty());
        }
        assert_eq!(THEMES.len(), 13);
    }
}
