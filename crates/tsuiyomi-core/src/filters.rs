//! Source filter trees exchanged with the extension host.
//!
//! Mihon filters are only applied to search requests. Popular and latest listings ignore
//! them, matching `GetRemoteManga` in upstream Mihon.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceFilter {
    Header {
        #[serde(default)]
        name: String,
    },
    Separator {
        #[serde(default)]
        name: String,
    },
    Checkbox {
        #[serde(default)]
        name: String,
        #[serde(default)]
        checked: bool,
    },
    Tristate {
        #[serde(default)]
        name: String,
        #[serde(default, rename = "triState")]
        tri_state: i32,
    },
    Text {
        #[serde(default)]
        name: String,
        #[serde(default)]
        text: String,
    },
    Select {
        #[serde(default)]
        name: String,
        #[serde(default)]
        selected: i32,
        #[serde(default)]
        values: Vec<String>,
    },
    Sort {
        #[serde(default)]
        name: String,
        #[serde(default)]
        values: Vec<String>,
        #[serde(default, rename = "sortIndex")]
        index: Option<i32>,
        #[serde(default)]
        ascending: Option<bool>,
    },
    Group {
        #[serde(default)]
        name: String,
        #[serde(default)]
        children: Vec<SourceFilter>,
    },
    Unknown {
        #[serde(default)]
        name: String,
    },
}

impl SourceFilter {
    pub fn name(&self) -> &str {
        match self {
            Self::Header { name }
            | Self::Separator { name }
            | Self::Checkbox { name, .. }
            | Self::Tristate { name, .. }
            | Self::Text { name, .. }
            | Self::Select { name, .. }
            | Self::Sort { name, .. }
            | Self::Group { name, .. }
            | Self::Unknown { name } => name,
        }
    }

    pub fn is_selectable(&self) -> bool {
        !matches!(self, Self::Header { .. } | Self::Separator { .. })
    }

    pub fn display_value(&self) -> String {
        match self {
            Self::Checkbox { checked, .. } => {
                if *checked {
                    "on".to_owned()
                } else {
                    "off".to_owned()
                }
            }
            Self::Tristate { tri_state, .. } => match tri_state {
                1 => "include".to_owned(),
                2 => "exclude".to_owned(),
                _ => "ignore".to_owned(),
            },
            Self::Text { text, .. } => text.clone(),
            Self::Select {
                selected, values, ..
            } => values
                .get(*selected as usize)
                .cloned()
                .unwrap_or_else(|| selected.to_string()),
            Self::Sort {
                index,
                ascending,
                values,
                ..
            } => {
                let label = index
                    .and_then(|index| values.get(index as usize))
                    .cloned()
                    .unwrap_or_else(|| "default".to_owned());
                let arrow = match ascending {
                    Some(false) => "↓",
                    _ => "↑",
                };
                format!("{label} {arrow}")
            }
            Self::Group { children, .. } => format!("{} filters", children.len()),
            Self::Header { .. } | Self::Separator { .. } => String::new(),
            Self::Unknown { .. } => "unsupported".to_owned(),
        }
    }

    /// Cycle checkbox/tri-state values. Returns whether the filter changed.
    pub fn toggle(&mut self) -> bool {
        match self {
            Self::Checkbox { checked, .. } => {
                *checked = !*checked;
                true
            }
            Self::Tristate { tri_state, .. } => {
                *tri_state = (*tri_state + 1) % 3;
                true
            }
            _ => false,
        }
    }

    /// Adjust select and tri-state values with left/right.
    pub fn adjust(&mut self, delta: i32) -> bool {
        match self {
            Self::Tristate { tri_state, .. } => {
                *tri_state = (*tri_state + delta).rem_euclid(3);
                true
            }
            Self::Select {
                selected, values, ..
            } => {
                if values.is_empty() {
                    return false;
                }
                *selected = (*selected + delta).rem_euclid(values.len() as i32);
                true
            }
            Self::Sort {
                index,
                ascending,
                values,
                ..
            } => {
                if delta < 0 {
                    *ascending = Some(false);
                } else if delta > 0 {
                    *ascending = Some(true);
                } else if !values.is_empty() {
                    let next = index.map_or(0, |index| (index + 1).rem_euclid(values.len() as i32));
                    *index = Some(next);
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_filters_and_values() {
        let filters: Vec<SourceFilter> = serde_json::from_str(
            r#"[
                {"type":"header","name":"Tags"},
                {"type":"checkbox","name":"Completed","checked":true},
                {"type":"tristate","name":"Adult","triState":2},
                {"type":"text","name":"Author","text":"Urasawa"},
                {"type":"select","name":"Order","selected":1,"values":["Popular","Latest"]},
                {"type":"sort","name":"Sort","sortIndex":0,"ascending":false,"values":["Name","Date"]},
                {"type":"group","name":"More","children":[{"type":"checkbox","name":"Oneshot"}]}
            ]"#,
        )
        .unwrap();

        assert_eq!(filters[1].display_value(), "on");
        assert_eq!(filters[2].display_value(), "exclude");
        assert_eq!(filters[3].display_value(), "Urasawa");
        assert_eq!(filters[4].display_value(), "Latest");
        assert_eq!(filters[5].display_value(), "Name ↓");
        assert!(!filters[0].is_selectable());
        assert!(filters[6].is_selectable());
    }

    #[test]
    fn cycling_and_adjusting_filters_works() {
        let mut tri = SourceFilter::Tristate {
            name: "Adult".to_owned(),
            tri_state: 0,
        };
        assert!(tri.toggle());
        assert_eq!(tri.display_value(), "include");
        assert!(tri.adjust(-1));
        assert_eq!(tri.display_value(), "ignore");

        let mut select = SourceFilter::Select {
            name: "Order".to_owned(),
            selected: 1,
            values: vec!["Popular".to_owned(), "Latest".to_owned()],
        };
        assert!(select.adjust(1));
        assert_eq!(select.display_value(), "Popular");

        let mut sort = SourceFilter::Sort {
            name: "Sort".to_owned(),
            values: vec!["Name".to_owned()],
            index: Some(0),
            ascending: Some(true),
        };
        assert!(sort.adjust(-1));
        assert_eq!(sort.display_value(), "Name ↓");
    }

    #[test]
    fn serializes_back_to_the_host_shape() {
        let filters = vec![SourceFilter::Checkbox {
            name: "Completed".to_owned(),
            checked: true,
        }];
        let value = serde_json::to_value(&filters).unwrap();
        assert_eq!(value[0]["type"], "checkbox");
        assert_eq!(value[0]["checked"], true);
    }
}
