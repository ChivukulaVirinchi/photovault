//! Friendly camera-name resolution.
//!
//! EXIF Make/Model tags often hold internal codenames (`M2007J20CI`,
//! `ILCE-7M3`, `iPhone15,2`) — useless to a human reading the photo
//! info panel. This module turns them into the marketing names users
//! actually recognise.
//!
//! Strategy, in priority order:
//! 1. **Pattern-based transforms** for prefixes we can decode without
//!    a lookup (Sony ILCE → α-series, Canon EOS → keep, Apple iPhone
//!    DeviceCode → marketing name from a small static map).
//! 2. **Curated codename table** for the long tail of Android codenames
//!    (Xiaomi, OnePlus, Realme, etc.) where the EXIF Model is an
//!    unreadable internal ID.
//! 3. **Optional CSV lookup** at `data/device_codenames.csv`, loaded
//!    lazily once. Lets users (or `setup_assets.sh`) drop in a richer
//!    list — same shape as Google Play's public supported-devices CSV
//!    (`retail_branding,marketing_name,device,model`) — without us
//!    bundling a 700 KB file.
//! 4. **Smart fallback**: combine `make` + `model`, dedup if model
//!    already starts with the make, title-case where it looks like a
//!    codename, leave alone where it doesn't.
//!
//! Everything is pure-Rust, offline, and never makes a network call.

use std::sync::OnceLock;

/// Resolve `make` + `model` (raw EXIF) into a friendly display name.
/// Returns `None` only when both are empty.
pub fn friendly_camera_name(make: Option<&str>, model: Option<&str>) -> Option<String> {
    let make = make.map(str::trim).filter(|s| !s.is_empty());
    let model = model.map(str::trim).filter(|s| !s.is_empty());

    if make.is_none() && model.is_none() {
        return None;
    }

    // Try pattern-based transforms first — they're free and cover
    // nearly every consumer camera.
    if let Some(name) = pattern_transform(make, model) {
        return Some(name);
    }

    // Then curated codename table.
    if let Some(model) = model {
        if let Some(name) = lookup_curated(model) {
            return Some(name.to_string());
        }
        // Optional CSV — only loaded once, on first miss.
        if let Some(name) = lookup_csv(model) {
            return Some(name);
        }
    }

    // Smart fallback.
    Some(combine_make_model(make, model))
}

fn pattern_transform(make: Option<&str>, model: Option<&str>) -> Option<String> {
    let model = model?;

    // Apple iPhone DeviceCode (e.g. "iPhone15,2") → marketing name.
    if let Some(name) = iphone_marketing_name(model) {
        return Some(name.to_string());
    }

    // Sony ILCE-7M3 → "Sony α7 III"; ILCE-7RM4 → "Sony α7R IV"; ILCE-9M2 → "Sony α9 II".
    if let Some(rest) = model.strip_prefix("ILCE-") {
        if let Some(pretty) = sony_alpha_pretty(rest) {
            return Some(format!("Sony α{}", pretty));
        }
    }
    // Sony DSC-RX100M7 → "Sony RX100 VII"
    if let Some(rest) = model.strip_prefix("DSC-") {
        if let Some(pretty) = sony_rx_pretty(rest) {
            return Some(format!("Sony {}", pretty));
        }
        return Some(format!("Sony {}", rest));
    }

    // Canon EOS already friendly — just prepend "Canon" if missing.
    if model.starts_with("EOS ") || model.starts_with("EOS-") || model.starts_with("Canon EOS") {
        if model.starts_with("Canon ") {
            return Some(model.to_string());
        }
        return Some(format!("Canon {}", model));
    }

    // Nikon Z / D / Coolpix already friendly.
    let make_lower = make.unwrap_or("").to_ascii_lowercase();
    if make_lower.contains("nikon") {
        if model.starts_with("Nikon ") {
            return Some(model.to_string());
        }
        if model.starts_with('Z')
            || model.starts_with('D')
            || model.to_ascii_lowercase().starts_with("coolpix")
        {
            return Some(format!("Nikon {}", model));
        }
    }

    // Fujifilm X-T4 etc. already friendly.
    if (make_lower.contains("fujifilm") || make_lower.contains("fuji"))
        && (model.starts_with("X-")
            || model.starts_with("GFX")
            || model.starts_with("FinePix")
            || model.starts_with("Fujifilm"))
    {
        return Some(format!(
            "Fujifilm {}",
            model.trim_start_matches("Fujifilm ")
        ));
    }

    // Pixel: "Pixel 7 Pro", already friendly. Prepend Google.
    if model.starts_with("Pixel ") {
        return Some(format!("Google {}", model));
    }

    None
}

/// Sony ILCE codename → α-series suffix. e.g. "7M3" → "7 III",
/// "7RM4" → "7R IV", "6400" → "6400", "9M2" → "9 II".
fn sony_alpha_pretty(suffix: &str) -> Option<&'static str> {
    Some(match suffix {
        "7" => "7",
        "7M2" => "7 II",
        "7M3" => "7 III",
        "7M4" => "7 IV",
        "7M5" => "7 V",
        "7R" => "7R",
        "7RM2" => "7R II",
        "7RM3" => "7R III",
        "7RM4" => "7R IV",
        "7RM5" => "7R V",
        "7S" => "7S",
        "7SM2" => "7S II",
        "7SM3" => "7S III",
        "7C" => "7C",
        "7CM2" => "7C II",
        "9" => "9",
        "9M2" => "9 II",
        "9M3" => "9 III",
        "1" => "1",
        "5100" => "5100",
        "6000" => "6000",
        "6100" => "6100",
        "6300" => "6300",
        "6400" => "6400",
        "6500" => "6500",
        "6600" => "6600",
        "6700" => "6700",
        _ => return None,
    })
}

fn sony_rx_pretty(suffix: &str) -> Option<&'static str> {
    Some(match suffix {
        "RX100" => "RX100",
        "RX100M2" => "RX100 II",
        "RX100M3" => "RX100 III",
        "RX100M4" => "RX100 IV",
        "RX100M5" => "RX100 V",
        "RX100M5A" => "RX100 VA",
        "RX100M6" => "RX100 VI",
        "RX100M7" => "RX100 VII",
        "RX10" => "RX10",
        "RX10M2" => "RX10 II",
        "RX10M3" => "RX10 III",
        "RX10M4" => "RX10 IV",
        "RX1" => "RX1",
        "RX1RM2" => "RX1R II",
        _ => return None,
    })
}

/// Translate Apple's `iPhoneN,M` device identifier into the marketing name.
fn iphone_marketing_name(model: &str) -> Option<&'static str> {
    Some(match model {
        // iPhone (1st)
        "iPhone1,1" => "iPhone",
        "iPhone1,2" => "iPhone 3G",
        "iPhone2,1" => "iPhone 3GS",
        "iPhone3,1" | "iPhone3,2" | "iPhone3,3" => "iPhone 4",
        "iPhone4,1" => "iPhone 4S",
        "iPhone5,1" | "iPhone5,2" => "iPhone 5",
        "iPhone5,3" | "iPhone5,4" => "iPhone 5c",
        "iPhone6,1" | "iPhone6,2" => "iPhone 5s",
        "iPhone7,1" => "iPhone 6 Plus",
        "iPhone7,2" => "iPhone 6",
        "iPhone8,1" => "iPhone 6s",
        "iPhone8,2" => "iPhone 6s Plus",
        "iPhone8,4" => "iPhone SE",
        "iPhone9,1" | "iPhone9,3" => "iPhone 7",
        "iPhone9,2" | "iPhone9,4" => "iPhone 7 Plus",
        "iPhone10,1" | "iPhone10,4" => "iPhone 8",
        "iPhone10,2" | "iPhone10,5" => "iPhone 8 Plus",
        "iPhone10,3" | "iPhone10,6" => "iPhone X",
        "iPhone11,2" => "iPhone XS",
        "iPhone11,4" | "iPhone11,6" => "iPhone XS Max",
        "iPhone11,8" => "iPhone XR",
        "iPhone12,1" => "iPhone 11",
        "iPhone12,3" => "iPhone 11 Pro",
        "iPhone12,5" => "iPhone 11 Pro Max",
        "iPhone12,8" => "iPhone SE (2nd gen)",
        "iPhone13,1" => "iPhone 12 mini",
        "iPhone13,2" => "iPhone 12",
        "iPhone13,3" => "iPhone 12 Pro",
        "iPhone13,4" => "iPhone 12 Pro Max",
        "iPhone14,2" => "iPhone 13 Pro",
        "iPhone14,3" => "iPhone 13 Pro Max",
        "iPhone14,4" => "iPhone 13 mini",
        "iPhone14,5" => "iPhone 13",
        "iPhone14,6" => "iPhone SE (3rd gen)",
        "iPhone14,7" => "iPhone 14",
        "iPhone14,8" => "iPhone 14 Plus",
        "iPhone15,2" => "iPhone 14 Pro",
        "iPhone15,3" => "iPhone 14 Pro Max",
        "iPhone15,4" => "iPhone 15",
        "iPhone15,5" => "iPhone 15 Plus",
        "iPhone16,1" => "iPhone 15 Pro",
        "iPhone16,2" => "iPhone 15 Pro Max",
        "iPhone17,1" => "iPhone 16 Pro",
        "iPhone17,2" => "iPhone 16 Pro Max",
        "iPhone17,3" => "iPhone 16",
        "iPhone17,4" => "iPhone 16 Plus",
        _ => return None,
    })
}

/// Curated codenames that aren't already mapped by patterns.
/// Covers the most common Xiaomi / Realme / OnePlus / Oppo / Samsung
/// internal IDs photographers see in their EXIF.
fn lookup_curated(model: &str) -> Option<&'static str> {
    // Case-insensitive match to forgive `m2007j20ci` / `M2007J20CI`.
    let upper = model.to_ascii_uppercase();
    Some(match upper.as_str() {
        // Xiaomi / Poco / Redmi (most common offenders)
        "M2007J20CI" | "M2007J20CG" | "M2007J20CT" => "Xiaomi Poco X3 NFC",
        "M2007J17G" | "M2007J17I" => "Xiaomi Mi 10T Lite",
        "M2007J3SY" | "M2007J3SI" => "Xiaomi Mi 10T Pro",
        "M2007J3SG" => "Xiaomi Mi 10T",
        "M2102J20SG" | "M2102J20SI" => "Xiaomi Poco X3 Pro",
        "M2102J2SG" | "M2102J2SI" => "Xiaomi Mi 11i",
        "M2102K1G" | "M2102K1AC" => "Xiaomi Mi 11",
        "M2103K19PG" | "M2103K19PI" => "Xiaomi Redmi Note 10",
        "M2101K6G" | "M2101K6P" | "M2101K6R" => "Xiaomi Redmi Note 10 Pro",
        "M2010J19SG" | "M2010J19SI" | "M2010J19CI" => "Xiaomi Redmi 9T",
        "21091116UG" | "21091116UI" => "Xiaomi Redmi Note 11 Pro",
        "2201116TG" | "2201116TI" => "Xiaomi Redmi Note 11",
        "2201117TG" | "2201117TI" => "Xiaomi Redmi Note 11S",
        "2203121C" | "2203129G" => "Xiaomi 12",
        "2201123G" | "2201122G" => "Xiaomi 12 Pro",
        "2210132C" | "2210132G" => "Xiaomi 13",
        "2211133G" | "2211133C" => "Xiaomi 13 Pro",
        "23049PCD8G" | "23049PCD8I" => "Xiaomi 13 Lite",
        "23116PN5BC" | "23116PN5BG" => "Xiaomi 14",
        "23117RA68G" | "23127PN0CG" => "Xiaomi 14 Pro",

        // OnePlus
        "GM1900" | "GM1903" | "GM1905" | "GM1911" | "GM1913" | "GM1915" | "GM1917" => "OnePlus 7",
        "GM1910" | "GM1911A" | "GM1913A" | "GM1917A" => "OnePlus 7 Pro",
        "HD1900" | "HD1901" | "HD1903" | "HD1905" | "HD1907" => "OnePlus 7T",
        "HD1910" | "HD1911" | "HD1913" => "OnePlus 7T Pro",
        "IN2010" | "IN2013" | "IN2015" | "IN2017" | "IN2019" => "OnePlus 8",
        "IN2020" | "IN2023" | "IN2025" => "OnePlus 8 Pro",
        "KB2000" | "KB2001" | "KB2003" | "KB2005" | "KB2007" => "OnePlus 8T",
        "LE2110" | "LE2111" | "LE2113" | "LE2115" | "LE2117" | "LE2119" | "LE2121" => "OnePlus 9",
        "LE2120" | "LE2123" | "LE2125" | "LE2127" => "OnePlus 9 Pro",
        "NE2210" | "NE2211" | "NE2213" | "NE2215" => "OnePlus 10 Pro",
        "PHB110" | "PHB120" => "OnePlus 11",
        "AC2001" | "AC2003" => "OnePlus Nord",
        "DN2101" | "DN2103" => "OnePlus Nord 2",

        // Realme
        "RMX2202" | "RMX2200" => "Realme 7 Pro",
        "RMX3081" | "RMX3085" => "Realme 8 Pro",
        "RMX3151" => "Realme 8",
        "RMX3370" | "RMX3371" => "Realme GT",
        "RMX3711" | "RMX3712" => "Realme GT Neo 3",

        // Oppo
        "CPH2173" | "CPH2179" => "Oppo Find X3 Pro",
        "PEXM00" => "Oppo Find X5 Pro",

        // Samsung Galaxy (when EXIF carries the model code rather than name)
        "SM-G991B" | "SM-G991U" | "SM-G991N" => "Samsung Galaxy S21",
        "SM-G996B" | "SM-G996U" | "SM-G996N" => "Samsung Galaxy S21+",
        "SM-G998B" | "SM-G998U" | "SM-G998N" => "Samsung Galaxy S21 Ultra",
        "SM-S901B" | "SM-S901U" | "SM-S901N" => "Samsung Galaxy S22",
        "SM-S906B" | "SM-S906U" | "SM-S906N" => "Samsung Galaxy S22+",
        "SM-S908B" | "SM-S908U" | "SM-S908N" => "Samsung Galaxy S22 Ultra",
        "SM-S911B" | "SM-S911U" | "SM-S911N" => "Samsung Galaxy S23",
        "SM-S916B" | "SM-S916U" | "SM-S916N" => "Samsung Galaxy S23+",
        "SM-S918B" | "SM-S918U" | "SM-S918N" => "Samsung Galaxy S23 Ultra",
        "SM-S921B" | "SM-S921U" | "SM-S921N" => "Samsung Galaxy S24",
        "SM-S926B" | "SM-S926U" | "SM-S926N" => "Samsung Galaxy S24+",
        "SM-S928B" | "SM-S928U" | "SM-S928N" => "Samsung Galaxy S24 Ultra",

        // Asus
        "ASUS_AI2202" => "Asus ROG Phone 6",
        _ => return None,
    })
}

/// Optional CSV lookup. Only loaded on first miss; CSV must live at
/// `data/device_codenames.csv` next to the binary or working dir.
/// Same column shape as Google Play's public list:
/// `retail_branding,marketing_name,device,model`.
fn lookup_csv(model: &str) -> Option<String> {
    static MAP: OnceLock<Option<std::collections::HashMap<String, String>>> = OnceLock::new();
    let map = MAP.get_or_init(load_csv);
    map.as_ref()?.get(&model.to_ascii_uppercase()).cloned()
}

fn load_csv() -> Option<std::collections::HashMap<String, String>> {
    let candidates = [
        std::path::PathBuf::from("data/device_codenames.csv"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("data/device_codenames.csv")))
            .unwrap_or_default(),
    ];
    for path in candidates.iter() {
        if !path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut map = std::collections::HashMap::new();
        for (i, line) in content.lines().enumerate() {
            if i == 0 && line.to_ascii_lowercase().contains("retail_branding") {
                continue; // header
            }
            let cells: Vec<&str> = line.split(',').collect();
            if cells.len() < 4 {
                continue;
            }
            let brand = cells[0].trim();
            let marketing = cells[1].trim();
            let model_code = cells[3].trim();
            if model_code.is_empty() || marketing.is_empty() {
                continue;
            }
            let display = if brand.is_empty()
                || marketing
                    .to_ascii_lowercase()
                    .starts_with(&brand.to_ascii_lowercase())
            {
                marketing.to_string()
            } else {
                format!("{} {}", brand, marketing)
            };
            map.insert(model_code.to_ascii_uppercase(), display);
        }
        return Some(map);
    }
    Some(std::collections::HashMap::new())
}

/// Last-resort: `make` and `model` joined, with cleanup.
fn combine_make_model(make: Option<&str>, model: Option<&str>) -> String {
    match (make, model) {
        (Some(make), Some(model)) => {
            // If model already starts with the make ("Canon Canon EOS R5"),
            // drop the duplicate.
            let model_lower = model.to_ascii_lowercase();
            let make_lower = make.to_ascii_lowercase();
            if model_lower.starts_with(&make_lower) {
                pretty_case(model)
            } else {
                format!("{} {}", make, pretty_case(model))
            }
        }
        (None, Some(model)) => pretty_case(model),
        (Some(make), None) => make.to_string(),
        (None, None) => String::new(),
    }
}

/// Title-case suspicious-looking codename strings; pass through anything
/// that already looks like prose (mixed case + spaces).
fn pretty_case(model: &str) -> String {
    let has_space = model.contains(' ');
    let all_upper = model
        .chars()
        .all(|c| !c.is_alphabetic() || c.is_uppercase());
    let all_lower = model
        .chars()
        .all(|c| !c.is_alphabetic() || c.is_lowercase());

    // Prose-y string already (mixed case + spaces) — keep it.
    if has_space && !all_upper && !all_lower {
        return model.to_string();
    }

    // Pure-codename: leave it but uppercase, since these are usually
    // marketing-side IDs (e.g. "ILCE-7M3" was already handled; we
    // only fall through here for ones not in the curated tables).
    if !has_space {
        return model.to_uppercase();
    }

    // Mixed: title-case each word.
    model
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xiaomi_codename() {
        assert_eq!(
            friendly_camera_name(Some("Xiaomi"), Some("M2007J20CI")).unwrap(),
            "Xiaomi Poco X3 NFC"
        );
        // case-insensitive
        assert_eq!(
            friendly_camera_name(Some("Xiaomi"), Some("m2007j20ci")).unwrap(),
            "Xiaomi Poco X3 NFC"
        );
    }

    #[test]
    fn sony_alpha() {
        assert_eq!(
            friendly_camera_name(Some("SONY"), Some("ILCE-7M3")).unwrap(),
            "Sony α7 III"
        );
        assert_eq!(
            friendly_camera_name(Some("SONY"), Some("ILCE-7RM4")).unwrap(),
            "Sony α7R IV"
        );
        assert_eq!(
            friendly_camera_name(Some("SONY"), Some("ILCE-6400")).unwrap(),
            "Sony α6400"
        );
    }

    #[test]
    fn iphone_devicecode() {
        assert_eq!(
            friendly_camera_name(Some("Apple"), Some("iPhone15,2")).unwrap(),
            "iPhone 14 Pro"
        );
        assert_eq!(
            friendly_camera_name(Some("Apple"), Some("iPhone16,2")).unwrap(),
            "iPhone 15 Pro Max"
        );
    }

    #[test]
    fn canon_eos_passes_through() {
        assert_eq!(
            friendly_camera_name(Some("Canon"), Some("EOS R5")).unwrap(),
            "Canon EOS R5"
        );
    }

    #[test]
    fn fallback_dedup() {
        // Make appears in model — don't duplicate.
        assert_eq!(
            friendly_camera_name(Some("Canon"), Some("Canon PowerShot S110")).unwrap(),
            "Canon PowerShot S110"
        );
    }

    #[test]
    fn unknown_codename_uppercase() {
        // Not in any table — uppercase the codename so it at least
        // looks consistent.
        assert_eq!(
            friendly_camera_name(Some("UnknownMaker"), Some("xyz123abc")).unwrap(),
            "UnknownMaker XYZ123ABC"
        );
    }

    #[test]
    fn empty_inputs() {
        assert!(friendly_camera_name(None, None).is_none());
        assert!(friendly_camera_name(Some(""), Some("")).is_none());
    }
}
