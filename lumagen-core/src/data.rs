//! Static data: map model (spec §7), prompt templates (spec §11),
//! material presets, description-check ingredients (spec §9), engine presets (§12).

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum MapId {
    Albedo,
    Roughness,
    Metallic,
    Normal,
    Displacement,
    Ao,
    Emission,
    Transparency,
}

impl MapId {
    pub const ALL: [MapId; 8] = [
        MapId::Albedo,
        MapId::Roughness,
        MapId::Metallic,
        MapId::Normal,
        MapId::Displacement,
        MapId::Ao,
        MapId::Emission,
        MapId::Transparency,
    ];
    pub const DERIVED: [MapId; 7] = [
        MapId::Roughness,
        MapId::Metallic,
        MapId::Normal,
        MapId::Displacement,
        MapId::Ao,
        MapId::Emission,
        MapId::Transparency,
    ];
    /// Position of this map in `ALL` / `MAPS`. Total: `MapId` is a closed enum and `ALL`
    /// is declared in the same order, so the index always exists — no fallible lookup.
    pub fn index(self) -> usize {
        match self {
            MapId::Albedo => 0,
            MapId::Roughness => 1,
            MapId::Metallic => 2,
            MapId::Normal => 3,
            MapId::Displacement => 4,
            MapId::Ao => 5,
            MapId::Emission => 6,
            MapId::Transparency => 7,
        }
    }
}

#[derive(Clone, Copy)]
pub struct MapDef {
    pub id: MapId,
    pub name: &'static str,
    pub group: &'static str,
    pub colorspace: &'static str,
    pub bits: u8,
    pub key: char,
}

pub const MAPS: [MapDef; 8] = [
    MapDef {
        id: MapId::Albedo,
        name: "Albedo",
        group: "Source",
        colorspace: "sRGB",
        bits: 8,
        key: '1',
    },
    MapDef {
        id: MapId::Roughness,
        name: "Roughness",
        group: "Surface response",
        colorspace: "Linear",
        bits: 8,
        key: '2',
    },
    MapDef {
        id: MapId::Metallic,
        name: "Metallic",
        group: "Surface response",
        colorspace: "Linear",
        bits: 8,
        key: '3',
    },
    MapDef {
        id: MapId::Normal,
        name: "Normal",
        group: "Geometry",
        colorspace: "Linear",
        bits: 8,
        key: '4',
    },
    MapDef {
        id: MapId::Displacement,
        name: "Displacement",
        group: "Geometry",
        colorspace: "Linear",
        bits: 16,
        key: '5',
    },
    MapDef {
        id: MapId::Ao,
        name: "Ambient Occlusion",
        group: "Geometry",
        colorspace: "Linear",
        bits: 8,
        key: '6',
    },
    MapDef {
        id: MapId::Emission,
        name: "Emission",
        group: "Optical",
        colorspace: "sRGB",
        bits: 8,
        key: '7',
    },
    MapDef {
        id: MapId::Transparency,
        name: "Transparency",
        group: "Optical",
        colorspace: "Linear",
        bits: 8,
        key: '8',
    },
];

/// The static definition for a map. Total: `MAPS` is ordered to match `MapId::index()`.
pub fn map_def(id: MapId) -> &'static MapDef {
    &MAPS[id.index()]
}

pub const MATERIAL_TYPES: [&str; 7] = ["Metal", "Painted", "Glass", "Fabric", "Stone", "Wood", "Custom"];

pub const PRICE_PER_IMAGE: f64 = 0.04;

// ── Engine presets (spec §12) ────────────────────────────────────────────

pub struct EnginePreset {
    pub name: &'static str,
    pub normal: &'static str,
    pub packing: &'static str,
    pub height: &'static str,
    pub format: &'static str,
}

pub const ENGINES: [EnginePreset; 6] = [
    EnginePreset {
        name: "Blender",
        normal: "OpenGL (Y+)",
        packing: "Separate maps",
        height: "16-bit PNG",
        format: "PNG",
    },
    EnginePreset {
        name: "Unreal",
        normal: "DirectX (Y−) · flip green",
        packing: "ORM  R=AO G=Rough B=Metal",
        height: "16-bit PNG",
        format: "PNG/TGA",
    },
    EnginePreset {
        name: "Unity URP",
        normal: "OpenGL (Y+)",
        packing: "MaskMap · Metallic+Smoothness",
        height: "16-bit PNG",
        format: "PNG",
    },
    EnginePreset {
        name: "Unity HDRP",
        normal: "OpenGL (Y+)",
        packing: "MaskMap (AO/Smooth/Metal)",
        height: "16-bit PNG",
        format: "PNG",
    },
    EnginePreset {
        name: "glTF",
        normal: "OpenGL (Y+)",
        packing: "metalRough packed",
        height: "baked to normal",
        format: "PNG",
    },
    EnginePreset {
        name: "Generic",
        normal: "OpenGL (Y+)",
        packing: "Separate maps",
        height: "16-bit PNG",
        format: "PNG",
    },
];

// ── Prompt templates (spec §11, from the script) ─────────────────────────

pub const BASE_TEMPLATE: &str = "Transform the supplied ALBEDO reference into a production PBR data texture.

LOCKED REGISTRATION — this is a data-map conversion, not a redesign:
- Preserve the exact square framing, orthographic top-down view, crop, scale, orientation, UV layout, component positions, panel boundaries, and proportions of the reference.
- Every feature in the output must correspond to the same feature at the same location in the albedo. Do not move, rotate, replace, simplify, or invent components.
- Edge-to-edge seamless tile. Opposite borders must continue naturally.
- Flat texture data only: no perspective, scene, background, directional light, cast shadow, vignette, depth of field, label, watermark, frame, or border.";

pub const FINAL_REQUIREMENT: &str =
    "FINAL REQUIREMENT: output one square map filling the canvas, immediately usable as texture data after resizing and channel conversion.";

pub const NEGATIVE_PROMPT: &str = "contact sheet, texture atlas, multiple maps in one image, labels, captions, borders, preview sphere, material ball, UI, infographic, poster, text, watermark, perspective view, angled camera, shadows, reflections, specular highlights, environmental lighting, bloom, glow spill, vignette, depth of field, changed panel placement, changed crop, changed scale, stretched texture, non-square output, baked AO in albedo, lighting baked into albedo, normal map containing macro displacement, displacement map containing micro-scratches";

pub const TPL_ALBEDO: &str = "Generate only the ALBEDO / BASE COLOR map. Include only surface color: coatings, bare-metal coloration, composites, muted hazard accents, oxidation, dirt, paint wear, subtle panel variation. Exclude shadows, highlights, reflections, ambient occlusion, bevel shading, and gloss/roughness. sRGB.";
pub const TPL_ROUGHNESS: &str = "Create a physically plausible ROUGHNESS MAP. Single-channel grayscale in RGB. Black = smooth/glossy, white = rough/matte. Infer from material identity: coating, bare metal, glass, rubber, plastic, dust, oxidation, fingerprints, wear. Preserve every material region from the albedo. Surface-property variation only — no lighting, highlights, shadows, AO, color, or new geometry.";
pub const TPL_METALLIC: &str = "Create a physically correct METALLIC MAP (metallic/roughness workflow). Grayscale in RGB. White = conductive bare metal, black = dielectric. Mostly near-binary. Painted metal is black where paint covers it; exposed hardware and rails white; glass, rubber, plastic, coatings, photovoltaic cells and diffusers black. Preserve exact boundaries. Do not infer metal from brightness. No lighting or bevel shading.";
pub const TPL_NORMAL: &str = "Create an OPENGL TANGENT-SPACE NORMAL MAP — MICRO DETAIL ONLY. Neutral flat is approximately RGB(128,128,255); +Y in the green channel (OpenGL). Owns ~90% of visible detail: small panel lines, screws, fasteners, shallow vents, tiny bevels, weld texture, scratches, micro-grid, pores. OMIT macro displacement: no large plate steps, deep bays, broad frames, or silhouette changes. Micro-features follow exact albedo positions. Output normal data only — no color, lighting, shadows, or AO.";
pub const TPL_DISPLACEMENT: &str = "Create a 16-bit DISPLACEMENT / HEIGHT map — MACRO SHAPES ONLY. Grayscale in RGB. White = highest/outward, black = deepest, mid-gray = base datum. Owns only the largest ~10%: armor-plate layering, deep bays, broad ribs, large housings, raised frames, deep recesses that affect silhouette. OMIT micro-greebles: panel lines, screws, scratches, small vents, pores, labels, paint wear. Broad heights match recognizable macro structures. No lighting, shadows, AO, or perspective.";
pub const TPL_AO: &str = "Create an AMBIENT OCCLUSION MAP. Single-channel grayscale in RGB. White = fully exposed to ambient light; darker = occluded contact shadow in crevices, seams, recesses, under raised plates, around fasteners, and inside vents. Derive only from albedo geometry and the height map. No directional lighting, cast shadows, color, roughness, or fine scratches. Broad soft gradients; preserve every structure location.";
pub const TPL_EMISSION: &str = "Create an RGB EMISSION MAP. Pure black everywhere that does not physically emit. Only components designed as emitters may be non-black: illuminated strips, lamps, status LEDs, powered displays. Preserve each emitter exact shape, location, and hue. No reflected light, bloom, glow halos, exposure haze, highlights, sunlight, bright paint, labels, or invented emitters.";
pub const TPL_TRANSPARENCY: &str = "Create an OPACITY / TRANSPARENCY MAP. Single-channel grayscale in RGB. White = fully opaque, black = fully transparent, grays = partial. Only physically see-through surfaces may be non-white: glass panes, clear polymer, mesh gaps, cut-outs. Solid metal, paint, rubber and diffusers stay pure white. Preserve exact region shapes. No lighting, tint color, refraction, shadows, or invented holes.";

pub fn default_map_template(id: MapId) -> &'static str {
    match id {
        MapId::Albedo => TPL_ALBEDO,
        MapId::Roughness => TPL_ROUGHNESS,
        MapId::Metallic => TPL_METALLIC,
        MapId::Normal => TPL_NORMAL,
        MapId::Displacement => TPL_DISPLACEMENT,
        MapId::Ao => TPL_AO,
        MapId::Emission => TPL_EMISSION,
        MapId::Transparency => TPL_TRANSPARENCY,
    }
}

pub fn single_map_suffix(id: MapId) -> String {
    format!(
        "Generate only the {} map. Ignore all other maps. Preserve the exact locked layout, scale, crop, framing, and feature placement from the reference.",
        map_def(id).name.to_uppercase()
    )
}

/// The exact concatenation the generator sends, with preview placeholders substituted for
/// `{resolution}`/`{seed}`.
pub fn compile_prompt(base: &str, final_req: &str, material_desc: &str, map_instructions: &str) -> String {
    compile_prompt_with(base, final_req, material_desc, map_instructions, 0, "4096")
}

/// Token-aware prompt compilation. If the base template contains `{material_description}` or
/// `{map_instructions}`, it is treated as a full template and those tokens are substituted in
/// place; otherwise the standard sections are appended. `{resolution}` and `{seed}` are always
/// substituted with their runtime values, so a token typed into any template expands instead of
/// being sent literally.
pub fn compile_prompt_with(base: &str, final_req: &str, material_desc: &str, map_instructions: &str, seed: u32, resolution: &str) -> String {
    let has_tokens = base.contains("{map_instructions}") || base.contains("{material_description}");
    let body = if has_tokens {
        base.replace("{material_description}", material_desc)
            .replace("{map_instructions}", map_instructions)
    } else {
        format!("{base}\n\nMATERIAL SEMANTICS:\n{material_desc}\n\n{map_instructions}")
    };
    let assembled = format!("{body}\n\n{final_req}");
    assembled.replace("{resolution}", resolution).replace("{seed}", &seed.to_string())
}

// ── Material presets (spec §10.4) ────────────────────────────────────────

pub struct MaterialPreset {
    pub slug: &'static str,
    pub desc: &'static str,
}

pub const PRESETS: [MaterialPreset; 7] = [
    MaterialPreset {
        slug: "painted_hull",
        desc: "Painted orbital-station hull: coated armor plates, exposed metal only at intentional chips or hardware, recessed seams, structural ribs, occasional warning markings, and sparse practical lights.",
    },
    MaterialPreset {
        slug: "bare_metal",
        desc: "Bare exposed structural spacecraft metal: conductive alloy plates, brackets, fasteners, welds, recesses, and restrained oxidation or machining variation.",
    },
    MaterialPreset {
        slug: "observation_glass",
        desc: "Reinforced observation glass: dielectric glass panes held by metallic frames, gaskets, clamps, structural borders, and only explicitly visible indicator lights.",
    },
    MaterialPreset {
        slug: "light_strip",
        desc: "Emissive station light-strip panels: luminous diffusers, non-metallic lenses, painted or bare metal housings, mounting channels, fasteners, and cable access.",
    },
    MaterialPreset {
        slug: "pipes_cables",
        desc: "Utility pipes and cable bundles: mixed bare metal pipes, painted conduits, rubber or polymer cables, clamps, couplers, junctions, brackets, and labels.",
    },
    MaterialPreset {
        slug: "conduits_vents",
        desc: "Dense station conduits, vents, and technical greebles: mixed metal housings, painted panels, grilles, ducts, fans, wiring, service boxes, and sparse indicators.",
    },
    MaterialPreset {
        slug: "solar_panel",
        desc: "Orbital-station solar panel: dark dielectric photovoltaic cells, conductive metal busbars and contacts, metallic support rails and frame, insulated backing, fasteners, hinges, and no self-emitting cell surface.",
    },
];

// ── Description-check ingredients (spec §9) ──────────────────────────────

pub struct Ingredient {
    #[allow(dead_code)]
    pub id: &'static str,
    pub label: &'static str,
    pub protects: &'static str,
    pub keywords: &'static [&'static str],
    pub addition: &'static str,
    pub reason: &'static str,
}

pub const INGREDIENTS: [Ingredient; 9] = [
    Ingredient {
        id: "metal",
        label: "Metal vs dielectric",
        protects: "Metallic · Roughness",
        keywords: &[
            "metal",
            "steel",
            "alloy",
            "conductive",
            "dielectric",
            "aluminum",
            "iron",
            "brass",
            "copper",
            "bare",
        ],
        addition: "note which regions are bare conductive metal vs painted or dielectric",
        reason: "Metallic and Roughness get guessed without it",
    },
    Ingredient {
        id: "coat",
        label: "Coatings / paint",
        protects: "Roughness",
        keywords: &["paint", "coat", "coated", "lacquer", "enamel", "matte", "satin", "gloss"],
        addition: "specify the paint finish (matte or satin) and where it covers metal",
        reason: "Roughness needs the coating vs bare split",
    },
    Ingredient {
        id: "glass",
        label: "Glass / transmission",
        protects: "Transparency",
        keywords: &["glass", "transparent", "translucent", "window", "pane", "lens", "clear", "acrylic"],
        addition: "name any transparent parts such as glass panes or clear lenses",
        reason: "Transparency stays flat white otherwise",
    },
    Ingredient {
        id: "rubber",
        label: "Rubber / plastic / composite",
        protects: "Roughness",
        keywords: &["rubber", "plastic", "polymer", "composite", "gasket", "seal", "silicone"],
        addition: "call out gaskets, seals and plastics so they are not read as metal",
        reason: "Non-metals get misclassified",
    },
    Ingredient {
        id: "wear",
        label: "Wear / oxidation / grime",
        protects: "Albedo · Roughness",
        keywords: &[
            "wear",
            "worn",
            "oxid",
            "rust",
            "grime",
            "dirt",
            "scratch",
            "scuff",
            "stain",
            "fingerprint",
            "weather",
            "patina",
        ],
        addition: "add wear such as edge scuffs, oxidation and grime near seams",
        reason: "The surface reads factory-clean without it",
    },
    Ingredient {
        id: "emit",
        label: "Emitters",
        protects: "Emission",
        keywords: &[
            "light",
            "led",
            "emissive",
            "glow",
            "strip",
            "lamp",
            "indicator",
            "luminous",
            "screen",
            "display",
        ],
        addition: "name the lights, LEDs or strips and their color",
        reason: "Emission comes back fully black otherwise",
    },
    Ingredient {
        id: "macro",
        label: "Macro relief",
        protects: "Displacement",
        keywords: &["plate", "bay", "rib", "step", "panel", "recess", "housing", "beam", "frame", "armor", "trench"],
        addition: "list the big forms (plates, bays, ribs, steps) for the height map",
        reason: "Displacement has nothing to build the silhouette from",
    },
    Ingredient {
        id: "micro",
        label: "Micro relief",
        protects: "Normal",
        keywords: &["seam", "screw", "bolt", "rivet", "vent", "grille", "weld", "mesh", "groove", "fastener"],
        addition: "list fine detail (seams, screws, grille, welds) for the normal map",
        reason: "The normal map goes flat without micro detail",
    },
    Ingredient {
        id: "seam",
        label: "Seamless / tiling",
        protects: "All maps",
        keywords: &["seamless", "tileable", "tiling"],
        addition: "confirm the texture is seamless and tileable",
        reason: "Edges may not continue across tiles",
    },
];

/// Lint a description against the ingredient list → booleans per ingredient.
pub fn detect_ingredients(desc: &str) -> Vec<bool> {
    let d = desc.to_lowercase();
    INGREDIENTS.iter().map(|ing| ing.keywords.iter().any(|k| d.contains(k))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_id_index_is_total_and_consistent() {
        // `index()` must be the position in `ALL` for every variant (the lookup never fails).
        for (i, id) in MapId::ALL.iter().enumerate() {
            assert_eq!(id.index(), i, "{:?} index mismatch", id);
        }
        // DERIVED is ALL minus Albedo.
        assert_eq!(MapId::DERIVED.len(), MapId::ALL.len() - 1);
        assert!(!MapId::DERIVED.contains(&MapId::Albedo));
    }

    #[test]
    fn compile_prompt_orders_sections() {
        let out = compile_prompt("BASE", "FINAL", "rusty hull", "emit only metal");
        let base = out.find("BASE").unwrap();
        let sem = out.find("MATERIAL SEMANTICS:").unwrap();
        let desc = out.find("rusty hull").unwrap();
        let instr = out.find("emit only metal").unwrap();
        let fin = out.find("FINAL").unwrap();
        assert!(base < sem && sem < desc && desc < instr && instr < fin, "section order wrong: {out}");
    }

    #[test]
    fn compile_prompt_with_substitutes_seed_and_resolution() {
        // Tokens anywhere in the templates expand to their runtime values instead of being sent
        // literally.
        let out = compile_prompt_with("base at {resolution} seed {seed}", "final {seed}", "desc", "map", 4821, "2048");
        assert!(out.contains("at 2048"), "resolution token not expanded: {out}");
        assert!(out.contains("seed 4821"), "seed token not expanded: {out}");
        assert!(!out.contains("{seed}") && !out.contains("{resolution}"), "a token was sent literally: {out}");
    }

    #[test]
    fn compile_prompt_with_token_template_does_not_double_insert() {
        // A base template that uses {material_description}/{map_instructions} is treated as a full
        // template — the sections aren't appended a second time.
        let out = compile_prompt_with("[{material_description}] :: [{map_instructions}]", "FINAL", "RUST", "ROUGH", 0, "4096");
        assert_eq!(out.matches("RUST").count(), 1, "description inserted twice: {out}");
        assert_eq!(out.matches("ROUGH").count(), 1, "map instructions inserted twice: {out}");
        assert!(!out.contains("MATERIAL SEMANTICS:"), "standard section appended despite tokens: {out}");
    }

    #[test]
    fn detect_ingredients_flags_known_keywords() {
        let hits = detect_ingredients("A seamless tileable painted metal surface");
        assert_eq!(hits.len(), INGREDIENTS.len());
        // The seamless/tileable ingredient must fire.
        let seamless_idx = INGREDIENTS.iter().position(|i| i.keywords.contains(&"seamless")).unwrap();
        assert!(hits[seamless_idx], "seamless keyword not detected");
        // An empty description detects nothing.
        assert!(detect_ingredients("").iter().all(|h| !h));
    }

    #[test]
    fn single_map_suffix_names_the_map() {
        for id in MapId::ALL {
            let suffix = single_map_suffix(id);
            let name = map_def(id).name.to_uppercase();
            assert!(suffix.contains(&name), "{:?} suffix missing its name", id);
            assert!(suffix.contains("Generate only"), "{:?} suffix missing directive", id);
        }
    }
}
