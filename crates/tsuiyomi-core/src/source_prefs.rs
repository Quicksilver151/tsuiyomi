//! Configurable-source preferences exposed by the extension host.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourcePreferenceKind {
    Switch,
    Text,
    List,
    Multi,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourcePreference {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(rename = "type")]
    pub kind: SourcePreferenceKind,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

impl SourcePreference {
    pub fn switch_value(&self) -> bool {
        self.value.as_deref() == Some("true")
    }

    pub fn display_value(&self) -> String {
        match self.kind {
            SourcePreferenceKind::Switch => {
                if self.switch_value() {
                    "on".to_owned()
                } else {
                    "off".to_owned()
                }
            }
            SourcePreferenceKind::Text | SourcePreferenceKind::Unknown => {
                self.value.clone().unwrap_or_default()
            }
            SourcePreferenceKind::List => self
                .values
                .iter()
                .position(|value| Some(value) == self.value.as_ref())
                .and_then(|index| self.labels.get(index).or_else(|| self.values.get(index)))
                .cloned()
                .or_else(|| self.value.clone())
                .unwrap_or_default(),
            SourcePreferenceKind::Multi => self.value.clone().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourcePreferences {
    #[serde(default, rename = "sourceId")]
    pub source_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub configurable: bool,
    #[serde(default)]
    pub prefs: Vec<SourcePreference>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcePreferenceValue {
    Bool(bool),
    Text(String),
}

impl SourcePreferenceValue {
    fn as_json(&self) -> Value {
        match self {
            Self::Bool(value) => json!(value),
            Self::Text(value) => json!(value),
        }
    }
}

pub fn preference_body(values: &BTreeMap<String, SourcePreferenceValue>) -> Value {
    Value::Object(
        values
            .iter()
            .map(|(key, value)| (key.clone(), value.as_json()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_switch_list_and_text_preferences() {
        let prefs: SourcePreferences = serde_json::from_str(
            r#"{
                "sourceId": "7",
                "name": "Demo",
                "configurable": true,
                "prefs": [
                    {"key":"nsfw","title":"Show NSFW","type":"switch","value":"true"},
                    {"key":"lang","title":"Language","type":"list","value":"en",
                     "values":["en","ja"],"labels":["English","Japanese"]},
                    {"key":"token","title":"Token","type":"text","value":"abc"},
                    {"key":"tags","title":"Tags","type":"multi","value":"action,drama"}
                ]
            }"#,
        )
        .unwrap();

        assert!(prefs.configurable);
        assert_eq!(prefs.prefs[0].display_value(), "on");
        assert_eq!(prefs.prefs[1].display_value(), "English");
        assert_eq!(prefs.prefs[2].display_value(), "abc");
        assert_eq!(prefs.prefs[3].display_value(), "action,drama");
    }

    #[test]
    fn builds_json_bodies_for_switch_and_text_values() {
        let mut values = BTreeMap::new();
        values.insert("nsfw".to_owned(), SourcePreferenceValue::Bool(false));
        values.insert(
            "token".to_owned(),
            SourcePreferenceValue::Text("xyz".to_owned()),
        );

        let body = preference_body(&values);

        assert_eq!(body["nsfw"], json!(false));
        assert_eq!(body["token"], json!("xyz"));
    }
}
