//! App themes. Generated from one list together with Android's `ui/theme/Themes.kt`: keep the two
//! the same (same ids, names and colours). The theme follows the account through the server.

pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub dark: bool,
    /// primary, on_primary, primary_c, on_primary_c, secondary, secondary_c, on_secondary_c, bg, on_bg, sc_low, sc, sc_high, sc_highest, on_surface_v, outline, outline_v, error
    pub colors: [&'static str; 17],
}

pub const CSS_VARS: [&str; 17] = ["--primary", "--on-primary", "--primary-c", "--on-primary-c", "--secondary", "--secondary-c", "--on-secondary-c", "--bg", "--on-bg", "--sc-low", "--sc", "--sc-high", "--sc-highest", "--on-surface-v", "--outline", "--outline-v", "--error"];

pub const THEMES: [Theme; 12] = [
    Theme { id: "midnight", name: "Midnight", dark: true, colors: ["#B388FF", "#000000", "#2A1748", "#EADDFF", "#D0BCFF", "#231B30", "#EADDFF", "#000000", "#FFFFFF", "#09080C", "#0E0C13", "#15121B", "#1C1824", "#B9B3C6", "#4A4257", "#2A2433", "#FF6E7F"] },
    Theme { id: "daylight", name: "Daylight", dark: false, colors: ["#0091BE", "#FFFFFF", "#CCF2FF", "#00313F", "#00A6C8", "#DDF6FC", "#00313F", "#F6FBFD", "#0E1A1F", "#F0F7FA", "#FFFFFF", "#E9F3F7", "#DDEBF0", "#4B5E66", "#8AA0A8", "#CFDDE2", "#C62839"] },
    Theme { id: "forest", name: "Forest", dark: true, colors: ["#7BD88F", "#00210B", "#173D22", "#C8F5D2", "#A8D5B2", "#1A2A1E", "#D3EFD9", "#050806", "#F1F7F2", "#0A0F0B", "#0F1610", "#151E17", "#1B261D", "#AFC2B3", "#435346", "#253028", "#FF6E7F"] },
    Theme { id: "ocean", name: "Ocean", dark: true, colors: ["#5CC8FF", "#002233", "#0F3247", "#CDEBFF", "#9FD0EC", "#152634", "#D3ECFA", "#03070B", "#EFF6FB", "#080D12", "#0C141B", "#121C25", "#18242F", "#A9BCCB", "#3F5162", "#22303C", "#FF6E7F"] },
    Theme { id: "sunset", name: "Sunset", dark: true, colors: ["#FF9E6B", "#331100", "#4A2214", "#FFDCCB", "#FFB3C7", "#3A1A24", "#FFD9E2", "#0A0605", "#FBF2EE", "#100A09", "#17100E", "#1F1613", "#281C18", "#C9B3AA", "#5A463F", "#33261F", "#FF6E7F"] },
    Theme { id: "amber", name: "Amber", dark: true, colors: ["#FFC857", "#2B1D00", "#433210", "#FFE8B3", "#E6C98A", "#2E2614", "#F4E3BF", "#080705", "#F8F4EC", "#0E0C08", "#15120C", "#1D1912", "#252016", "#C7BCA5", "#57503E", "#312B1F", "#FF6E7F"] },
    Theme { id: "crimson", name: "Crimson", dark: true, colors: ["#FF6B6B", "#330000", "#4A1414", "#FFDAD6", "#FFB4A9", "#3A1716", "#FFDAD6", "#070404", "#FAF1F0", "#0E0808", "#150D0C", "#1D1312", "#261918", "#C8B0AD", "#5B4341", "#342422", "#FFB4AB"] },
    Theme { id: "mono", name: "Mono", dark: true, colors: ["#E0E0E0", "#000000", "#333333", "#F2F2F2", "#BDBDBD", "#262626", "#EEEEEE", "#000000", "#FFFFFF", "#0A0A0A", "#111111", "#191919", "#212121", "#B3B3B3", "#4D4D4D", "#2B2B2B", "#FF6E7F"] },
    Theme { id: "lavender", name: "Lavender", dark: false, colors: ["#6E4FC4", "#FFFFFF", "#E8DEFF", "#23005C", "#8B73D6", "#EFE9FC", "#23005C", "#FBF8FF", "#1C1A23", "#F4F0FC", "#FFFFFF", "#EDE7F8", "#E3DCF2", "#5B5568", "#968FA6", "#DCD5E8", "#BA1A1A"] },
    Theme { id: "rose", name: "Rose", dark: false, colors: ["#C2185B", "#FFFFFF", "#FFD9E4", "#3E0017", "#D35C88", "#FCE4EC", "#3E0017", "#FFF8F9", "#231418", "#FFF0F3", "#FFFFFF", "#FBE7EC", "#F5DCE3", "#6E5157", "#A88A91", "#EBD3D9", "#B3261E"] },
    Theme { id: "mint", name: "Mint", dark: false, colors: ["#1B8A4B", "#FFFFFF", "#C8F2D6", "#00210E", "#3FA36A", "#E3F6EA", "#00210E", "#F6FBF7", "#111C15", "#EEF7F1", "#FFFFFF", "#E6F2EA", "#D9EBDF", "#4E6355", "#8AA393", "#D0E2D6", "#BA1A1A"] },
    Theme { id: "slate", name: "Slate", dark: false, colors: ["#3F5F86", "#FFFFFF", "#D6E3F7", "#0B1D33", "#5D7799", "#E6EDF6", "#0B1D33", "#F7F9FC", "#161C24", "#EFF3F8", "#FFFFFF", "#E7ECF3", "#DCE3EC", "#56606E", "#8D98A6", "#D2DAE4", "#BA1A1A"] },
];

/// The theme with this id, or Midnight/Daylight from the old dark-mode switch.
pub fn find(id: &str, dark_fallback: bool) -> &'static Theme {
    THEMES.iter().find(|t| t.id == id).unwrap_or(if dark_fallback { &THEMES[0] } else { &THEMES[1] })
}

/// Inline CSS that sets every colour variable of the stylesheet.
pub fn css(t: &Theme) -> String {
    CSS_VARS.iter().zip(t.colors.iter()).map(|(k, v)| format!("{k}:{v};")).collect()
}
