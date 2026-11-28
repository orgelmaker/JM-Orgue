//! Portable console designs. Geometry is validated before storage or rendering;
//! audio, MIDI and machine-specific window placement do not belong to this model.
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Moet gelijk blijven aan CONSOLE_THEMES in ui/src/lib/consoleLayout.js.
const CONSOLE_THEMES: [&str; 7] = ["walnut", "ivory", "slate", "burgundy", "cream", "green", "blue"];
/// Moet gelijk blijven aan CONSOLE_APPEARANCES in ui/src/lib/consoleLayout.js.
const CONSOLE_APPEARANCES: [&str; 14] = [
    "rocker", "rocker-ivory", "rocker-wood", "drawknob", "drawknob-ivory", "piston", "tablet", "flat",
    "drawknob-ebony", "drawknob-rosewood", "oval-porcelain", "stopkey", "lit", "square-wood",
];

pub const MAX_CUSTOM_TEXTURE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CUSTOM_TEXTURE_BASE64: usize = ((MAX_CUSTOM_TEXTURE_BYTES + 2) / 3) * 4;

fn default_texture() -> String {
    "grain".into()
}
fn default_opacity() -> f64 {
    0.65
}
fn default_scale() -> f64 {
    100.0
}
fn is_default_texture(value: &String) -> bool {
    value == "grain"
}
fn is_default_opacity(value: &f64) -> bool {
    *value == default_opacity()
}
fn is_default_scale(value: &f64) -> bool {
    *value == default_scale()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsoleLayout {
    pub version: u32,
    pub enabled: bool,
    pub name: String,
    pub theme: String,
    #[serde(
        default = "default_texture",
        skip_serializing_if = "is_default_texture"
    )]
    pub texture: String,
    #[serde(
        default = "default_opacity",
        skip_serializing_if = "is_default_opacity"
    )]
    pub texture_opacity: f64,
    #[serde(default = "default_scale", skip_serializing_if = "is_default_scale")]
    pub texture_scale: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_texture: Option<ConsoleCustomTexture>,
    pub pages: Vec<ConsolePage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsoleCustomTexture {
    pub name: String,
    pub data_url: String,
}

impl ConsoleCustomTexture {
    pub fn validate(&self) -> Result<(), String> {
        if !text_ok(&self.name, 120, true) {
            return Err("Ongeldige naam van de consoletextuur".into());
        }
        let (prefix, encoded) = self
            .data_url
            .split_once(',')
            .ok_or("Ongeldige consoletextuur")?;
        if ![
            "data:image/png;base64",
            "data:image/jpeg;base64",
            "data:image/webp;base64",
        ]
        .contains(&prefix)
        {
            return Err("Gebruik een PNG-, JPEG- of WebP-afbeelding voor de consoletextuur".into());
        }
        if encoded.len() > MAX_CUSTOM_TEXTURE_BASE64 {
            return Err("De consoletextuur mag maximaal 2 MiB groot zijn".into());
        }
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "Ongeldige base64-afbeelding voor de consoletextuur")?;
        if decoded.len() > MAX_CUSTOM_TEXTURE_BYTES {
            return Err("De consoletextuur mag maximaal 2 MiB groot zijn".into());
        }
        let matches = match prefix {
            "data:image/png;base64" => decoded.starts_with(b"\x89PNG\r\n\x1a\n"),
            "data:image/jpeg;base64" => decoded.starts_with(b"\xff\xd8\xff"),
            "data:image/webp;base64" => {
                decoded.starts_with(b"RIFF") && decoded.get(8..12) == Some(&b"WEBP"[..])
            }
            _ => false,
        };
        if !matches {
            return Err(
                "De inhoud van de consoletextuur komt niet overeen met het afbeeldingstype".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsolePage {
    pub id: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub elements: Vec<ConsoleElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsoleElement {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub target_id: Option<String>,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub appearance: String,
}

fn text_ok(value: &str, maximum: usize, empty: bool) -> bool {
    (empty || !value.trim().is_empty())
        && value.chars().count() <= maximum
        && !value.chars().any(|c| c.is_control() && c != '\n')
}

impl ConsoleLayout {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Deze versie van het consoleontwerp wordt niet ondersteund".into());
        }
        if !text_ok(&self.name, 120, true)
            || !CONSOLE_THEMES.contains(&self.theme.as_str())
        {
            return Err("Ongeldige naam of stijl van het consoleontwerp".into());
        }
        if ![
            "grain", "oak", "burl", "ebony", "linen", "brushed", "plain", "custom",
        ]
        .contains(&self.texture.as_str())
            || !self.texture_opacity.is_finite()
            || !(0.0..=1.0).contains(&self.texture_opacity)
            || !self.texture_scale.is_finite()
            || !(25.0..=400.0).contains(&self.texture_scale)
        {
            return Err("Ongeldige textuurinstellingen van het consoleontwerp".into());
        }
        if let Some(image) = &self.custom_texture {
            image.validate()?;
        }
        if self.texture == "custom" && self.custom_texture.is_none() {
            return Err("Kies eerst een eigen afbeelding voor de consoletextuur".into());
        }
        if self.pages.is_empty() || self.pages.len() > 64 {
            return Err("Een consoleontwerp heeft 1 tot 64 pagina's".into());
        }
        let mut pages = HashSet::new();
        let mut elements = HashSet::new();
        for page in &self.pages {
            if !text_ok(&page.id, 128, false)
                || !pages.insert(page.id.as_str())
                || !text_ok(&page.name, 120, true)
            {
                return Err("Ongeldige of dubbele consolepagina".into());
            }
            if !page.width.is_finite()
                || !(640.0..=2400.0).contains(&page.width)
                || !page.height.is_finite()
                || !(400.0..=1600.0).contains(&page.height)
            {
                return Err("De consolepagina moet 640–2400 bij 400–1600 groot zijn".into());
            }
            if page.elements.len() > 512 {
                return Err("Een consolepagina heeft maximaal 512 onderdelen".into());
            }
            for element in &page.elements {
                if !text_ok(&element.id, 128, false)
                    || !elements.insert(element.id.as_str())
                    || !text_ok(&element.label, 160, true)
                {
                    return Err("Ongeldig of dubbel consoleonderdeel".into());
                }
                if !["stop", "coupler", "label"].contains(&element.kind.as_str())
                    || !CONSOLE_APPEARANCES
                    .contains(&element.appearance.as_str())
                {
                    return Err("Onbekend type consolebediening".into());
                }
                let target_ok = match element.kind.as_str() {
                    "label" => element.target_id.is_none(),
                    _ => element
                        .target_id
                        .as_deref()
                        .map(|s| text_ok(s, 256, false))
                        .unwrap_or(false),
                };
                if !target_ok {
                    return Err("Het consoleonderdeel heeft geen geldig register of koppel".into());
                }
                let minimum = if element.kind == "label" { 16.0 } else { 40.0 };
                let minimum_width = if element.kind == "label" { 16.0 } else { 48.0 };
                if ![element.x, element.y, element.width, element.height]
                    .iter()
                    .all(|v| v.is_finite())
                    || element.x < 0.0
                    || element.y < 0.0
                    || element.width < minimum_width
                    || element.height < minimum
                    || element.x + element.width > page.width
                    || element.y + element.height > page.height
                {
                    return Err(
                        "Een consoleonderdeel valt buiten de pagina of heeft een ongeldig formaat"
                            .into(),
                    );
                }
            }
        }
        if elements.len() > 4096 {
            return Err("Een consoleontwerp heeft maximaal 4096 onderdelen".into());
        }
        Ok(())
    }

    /// Missing source objects may remain in an old saved design for repair, but
    /// a newly applied enabled design must only address objects of this organ.
    pub fn validate_targets(
        &self,
        stops: &HashSet<&str>,
        couplers: &HashSet<&str>,
    ) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        for element in self.pages.iter().flat_map(|p| &p.elements) {
            let exists = match element.kind.as_str() {
                "stop" => element
                    .target_id
                    .as_deref()
                    .map(|s| stops.contains(s))
                    .unwrap_or(false),
                "coupler" => element
                    .target_id
                    .as_deref()
                    .map(|s| couplers.contains(s))
                    .unwrap_or(false),
                _ => true,
            };
            if !exists {
                return Err(format!("Register of koppel '{}' ontbreekt in dit orgel. Verwijder of vervang deze bediening voordat u het ontwerp toepast", element.label));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn layout() -> ConsoleLayout {
        ConsoleLayout {
            version: 1,
            enabled: true,
            name: "Eigen console".into(),
            theme: "walnut".into(),
            texture: default_texture(),
            texture_opacity: default_opacity(),
            texture_scale: default_scale(),
            custom_texture: None,
            pages: vec![ConsolePage {
                id: "main".into(),
                name: "Hoofdscherm".into(),
                width: 1200.0,
                height: 760.0,
                elements: vec![ConsoleElement {
                    id: "element-1".into(),
                    kind: "stop".into(),
                    target_id: Some("stop-1".into()),
                    label: "Prestant 8′".into(),
                    x: 40.0,
                    y: 80.0,
                    width: 104.0,
                    height: 92.0,
                    appearance: "rocker".into(),
                }],
            }],
        }
    }

    #[test]
    fn version_and_geometry_are_checked_before_storage() {
        let original = layout();
        assert!(original.validate().is_ok());
        for value in [f64::NAN, f64::INFINITY, -1.0, 1200.0] {
            let mut changed = original.clone();
            changed.pages[0].elements[0].x = value;
            assert!(changed.validate().is_err(), "x={value}");
        }
        let mut changed = original.clone();
        changed.version = 2;
        assert!(changed.validate().is_err());
        changed = original.clone();
        let duplicate = changed.pages[0].elements[0].clone();
        changed.pages[0].elements.push(duplicate);
        assert!(changed.validate().is_err());
        changed = original.clone();
        let duplicate = changed.pages[0].clone();
        changed.pages.push(duplicate);
        assert!(changed.validate().is_err());
        changed = original.clone();
        changed.pages[0].elements[0].kind = "script".into();
        assert!(changed.validate().is_err());
        changed = original.clone();
        changed.pages[0].elements[0].target_id = None;
        assert!(changed.validate().is_err());
    }

    #[test]
    fn missing_targets_can_be_repaired_or_disabled_but_not_activated() {
        let mut saved = layout();
        let empty = HashSet::new();
        assert!(saved.validate_targets(&empty, &empty).is_err());
        assert!(saved
            .validate_targets(&HashSet::from(["stop-1"]), &empty)
            .is_ok());
        saved.enabled = false;
        assert!(saved.validate_targets(&empty, &empty).is_ok());
        // Reading a saved design validates its format, without dropping objects
        // that disappeared in a sampleset update.
        assert!(saved.validate().is_ok());
    }

    #[test]
    fn layout_roundtrips_without_machine_specific_data() {
        let json = serde_json::to_string(&layout()).unwrap();
        assert_eq!(
            serde_json::from_str::<ConsoleLayout>(&json).unwrap(),
            layout()
        );
        assert!(!json.contains("window"));
    }

    #[test]
    fn old_designs_keep_their_default_texture_without_rewriting_fields() {
        let old = serde_json::to_value(layout()).unwrap();
        for key in [
            "texture",
            "texture_opacity",
            "texture_scale",
            "custom_texture",
        ] {
            assert!(old.get(key).is_none());
        }
        let loaded: ConsoleLayout = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(loaded.texture, "grain");
        assert_eq!(loaded.texture_opacity, 0.65);
        assert_eq!(loaded.texture_scale, 100.0);
        assert_eq!(serde_json::to_value(loaded).unwrap(), old);
    }

    fn texture(mime: &str, bytes: &[u8]) -> ConsoleCustomTexture {
        ConsoleCustomTexture {
            name: "Eigen textuur".into(),
            data_url: format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
        }
    }

    #[test]
    fn raster_textures_and_all_button_appearances_are_supported() {
        for (mime, bytes) in [
            ("image/png", b"\x89PNG\r\n\x1a\n".as_slice()),
            ("image/jpeg", b"\xff\xd8\xff\xd9".as_slice()),
            ("image/webp", b"RIFF\x04\0\0\0WEBP".as_slice()),
        ] {
            let mut design = layout();
            design.texture = "custom".into();
            design.custom_texture = Some(texture(mime, bytes));
            assert!(design.validate().is_ok(), "{mime}");
            assert_eq!(
                serde_json::from_str::<ConsoleLayout>(&serde_json::to_string(&design).unwrap())
                    .unwrap(),
                design
            );
        }
        for appearance in CONSOLE_APPEARANCES {
            let mut design = layout();
            design.pages[0].elements[0].appearance = appearance.into();
            assert!(design.validate().is_ok());
        }
    }

    #[test]
    fn texture_validation_rejects_bad_mime_magic_base64_size_and_ranges() {
        let png = b"\x89PNG\r\n\x1a\n";
        let invalid = [
            texture("image/svg+xml", b"<svg/>"),
            texture("image/jpeg", png),
            ConsoleCustomTexture {
                name: "Bad".into(),
                data_url: "data:image/png;base64,iVBORw0KGgo=!".into(),
            },
            ConsoleCustomTexture {
                name: "Bad".into(),
                data_url: "data:image/png;base64,iVBORw0KGgp=".into(),
            },
            texture("image/png", &vec![0; MAX_CUSTOM_TEXTURE_BYTES + 1]),
        ];
        for image in invalid {
            assert!(image.validate().is_err());
        }
        let mut boundary = vec![0; MAX_CUSTOM_TEXTURE_BYTES];
        boundary[..png.len()].copy_from_slice(png);
        assert!(texture("image/png", &boundary).validate().is_ok());
        let mut design = layout();
        design.texture = "custom".into();
        assert!(design.validate().is_err());
        design = layout();
        design.texture_opacity = 1.01;
        assert!(design.validate().is_err());
        design = layout();
        design.texture_scale = 24.9;
        assert!(design.validate().is_err());
        design = layout();
        design.custom_texture = Some(texture("image/jpeg", png));
        assert!(design.validate().is_err());
    }
}
