//! Prompt-pack export/import. A prompt pack is the editable prompt state — base template,
//! final requirement, negative, and per-map templates — serialized to `prompts.json`.
//! Export writes the current pack; import merges a file over the active pack.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::data::MapId;
use crate::error::{Error, Result};

/// The serializable prompt pack.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptPack {
    /// Base/system template.
    pub base_template: String,
    /// Final-requirement suffix.
    pub final_requirement: String,
    /// Negative prompt.
    pub negative_prompt: String,
    /// Per-map instruction templates (keyed by `MapId::index()`).
    pub map_templates: HashMap<String, String>,
}

impl PromptPack {
    /// Serialize to a JSON file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(Error::from)?;
        std::fs::write(path, json).map_err(Error::Io)
    }

    /// Load from a JSON file.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(Error::Io)?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    /// The template for a map, or `None` if the pack doesn't override it.
    pub fn template_for(&self, id: MapId) -> Option<&str> {
        self.map_templates.get(&key(id)).map(String::as_str)
    }

    /// Set the template for a map.
    pub fn set_template(&mut self, id: MapId, tpl: String) {
        self.map_templates.insert(key(id), tpl);
    }
}

/// The `map_templates` key for a map (its `MapId::index()` as a stable string).
fn key(id: MapId) -> String {
    id.index().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_pack_round_trip() {
        let mut pack = PromptPack {
            base_template: "BASE".into(),
            negative_prompt: "NEG".into(),
            ..Default::default()
        };
        pack.set_template(MapId::Normal, "normal tpl".into());
        let path = std::env::temp_dir().join("lumagen_prompts_test.json");
        pack.save(&path).expect("save");
        let loaded = PromptPack::load(&path).expect("load");
        assert_eq!(loaded.base_template, "BASE");
        assert_eq!(loaded.negative_prompt, "NEG");
        assert_eq!(loaded.template_for(MapId::Normal), Some("normal tpl"));
        let _ = std::fs::remove_file(&path);
    }
}
