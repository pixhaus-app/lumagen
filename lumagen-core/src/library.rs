//! Library scanning: discovers materials on disk. Two kinds of material are recognized: a
//! `.lumagen` document (one per material), and a *folder of loose maps* (an albedo +
//! suffixed derived maps like `*_roughness.png`), which "Import from folder" groups into a
//! material with a real coverage count.

use std::path::{Path, PathBuf};

use crate::data::{MAPS, MapId};
use crate::error::{Error, Result};

/// A material discovered on disk.
#[derive(Clone, Debug)]
pub struct ScannedMaterial {
    /// Material name (slug): the document stem or folder name.
    pub slug: String,
    /// Where it lives (the `.lumagen` file or the folder).
    pub path: PathBuf,
    /// Material class (from the document, or inferred).
    pub class: String,
    /// The project the material belongs to (from the document; empty for loose folders).
    pub project: String,
    /// How many of the 8 maps are present.
    pub coverage: u8,
    /// The map indices that are stale (derived map older than its albedo), if detectable.
    pub stale_index: Option<usize>,
    /// The seed stored in the document (0 for loose folders).
    pub seed: u32,
    /// Last-modified time of the backing file/folder, for recency sorting.
    pub modified: Option<std::time::SystemTime>,
}

/// Scan a directory for `.lumagen` documents. Returns one entry per readable document,
/// most recently modified first; unreadable files are skipped (logged via tracing, not fatal).
#[tracing::instrument(skip(dir), fields(dir = %dir.display()))]
pub fn scan_documents(dir: &Path) -> Result<Vec<ScannedMaterial>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(Error::Io)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lumagen") {
            continue;
        }
        match crate::document::Document::load(&path) {
            Ok(doc) => {
                let coverage = doc.maps.values().filter(|m| m.has_image).count() as u8;
                let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                out.push(ScannedMaterial {
                    slug: doc.material_name,
                    path,
                    class: doc.mat_type,
                    project: doc.project_name,
                    coverage,
                    stale_index: None,
                    seed: doc.seed,
                    modified,
                });
            }
            Err(e) => {
                tracing::warn!("skipping unreadable document {}: {e}", path.display());
            }
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(out)
}

/// Image extensions that count toward folder-import coverage; sidecar files (notes,
/// metadata) must not inflate the badge.
const IMPORT_IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "exr"];

/// The filename token identifying each map in a loose-map folder (case-insensitive).
fn map_token(id: MapId) -> &'static str {
    match id {
        MapId::Albedo => "albedo",
        MapId::Roughness => "roughness",
        MapId::Metallic => "metallic",
        MapId::Normal => "normal",
        MapId::Displacement => "displacement",
        MapId::Ao => "ao",
        MapId::Emission => "emission",
        MapId::Transparency => "transparency",
    }
}

/// Whether a lowercased file stem names a map: the token must be a whole `_`/`-`/space
/// delimited component, never a substring — "chaos" must not register AO, "normalized"
/// must not register Normal. Delimited components also keep `chaos_albedo_2k` matching.
fn stem_matches(stem: &str, token: &str) -> bool {
    stem.split(['_', '-', ' ']).any(|part| part == token)
}

/// Group a folder of loose map images into a material by matching map name tokens. Returns
/// `None` if no albedo is found (a folder without a source isn't a material). Coverage counts
/// how many of the 8 expected maps are present.
pub fn import_folder(dir: &Path) -> Result<Option<ScannedMaterial>> {
    let read = std::fs::read_dir(dir).map_err(Error::Io)?;
    let stems: Vec<String> = read
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let ext = path.extension().and_then(|x| x.to_str())?.to_lowercase();
            if !IMPORT_IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                return None;
            }
            path.file_stem().and_then(|s| s.to_str()).map(str::to_lowercase)
        })
        .collect();
    let has = |id: MapId| stems.iter().any(|stem| stem_matches(stem, map_token(id)));
    if !has(MapId::Albedo) {
        return Ok(None);
    }
    let coverage = MAPS.iter().filter(|d| has(d.id)).count() as u8;
    let slug = dir.file_name().and_then(|n| n.to_str()).unwrap_or("material").to_string();
    let modified = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
    Ok(Some(ScannedMaterial {
        slug,
        path: dir.to_path_buf(),
        class: "Custom".into(),
        project: String::new(),
        coverage,
        stale_index: None,
        seed: 0,
        modified,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    /// Unique per-process/per-test temp dir, removed on drop (including panic unwind) so
    /// concurrent suite runs can't race on a shared fixture path.
    struct TestDir(PathBuf);

    impl TestDir {
        fn new(test: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("lumagen_lib_{}_{test}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("create test dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn scan_documents_finds_saved_materials() {
        let dir = TestDir::new("scan");
        let doc = Document {
            material_name: "test_mat".into(),
            mat_type: "Metal".into(),
            ..Default::default()
        };
        doc.save(&dir.path().join("test_mat.lumagen")).expect("save");
        let found = scan_documents(dir.path()).expect("scan");
        assert!(found.iter().any(|m| m.slug == "test_mat" && m.class == "Metal"));
    }

    #[test]
    fn import_folder_groups_by_suffix() {
        let root = TestDir::new("import_groups");
        let dir = root.path().join("my_material");
        std::fs::create_dir_all(&dir).expect("mkdir");
        for f in ["albedo.png", "my_roughness.png", "normal.png"] {
            std::fs::write(dir.join(f), b"x").expect("write");
        }
        let m = import_folder(&dir).expect("import").expect("material found");
        assert_eq!(m.slug, "my_material");
        assert_eq!(m.coverage, 3);
    }

    #[test]
    fn import_folder_without_albedo_is_none() {
        let dir = TestDir::new("import_empty");
        std::fs::write(dir.path().join("roughness.png"), b"x").expect("write");
        assert!(import_folder(dir.path()).expect("import").is_none());
    }

    #[test]
    fn import_folder_ignores_substring_and_non_image_matches() {
        let dir = TestDir::new("import_tokens");
        for f in ["chaos_albedo.png", "normalized_grid.png", "roughness_notes.txt"] {
            std::fs::write(dir.path().join(f), b"x").expect("write");
        }
        let m = import_folder(dir.path()).expect("import").expect("material found");
        // Only the real albedo counts: "chaos" must not register AO, "normalized" must
        // not register Normal, and a .txt sidecar must not register Roughness.
        assert_eq!(m.coverage, 1);
    }
}
