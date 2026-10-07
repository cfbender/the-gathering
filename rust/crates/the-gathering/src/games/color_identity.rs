//! Canonical WUBRG ordering and community names for color combinations.

const ORDER: [char; 5] = ['W', 'U', 'B', 'R', 'G'];

/// Reorders identity letters into WUBRG order, dropping duplicates and unknown letters.
pub fn canonical(identity: &str) -> String {
    ORDER
        .iter()
        .filter(|color| identity.contains(**color))
        .collect()
}

/// The common name for a color combination, falling back to the canonical letters.
pub fn name(identity: &str) -> String {
    let canonical = canonical(identity);
    let named = match canonical.as_str() {
        "" => "Colorless",
        "W" => "Mono-White",
        "U" => "Mono-Blue",
        "B" => "Mono-Black",
        "R" => "Mono-Red",
        "G" => "Mono-Green",
        "WU" => "Azorius",
        "WB" => "Orzhov",
        "WR" => "Boros",
        "WG" => "Selesnya",
        "UB" => "Dimir",
        "UR" => "Izzet",
        "UG" => "Simic",
        "BR" => "Rakdos",
        "BG" => "Golgari",
        "RG" => "Gruul",
        "WUB" => "Esper",
        "WUR" => "Jeskai",
        "WUG" => "Bant",
        "WBR" => "Mardu",
        "WBG" => "Abzan",
        "WRG" => "Naya",
        "UBR" => "Grixis",
        "UBG" => "Sultai",
        "URG" => "Temur",
        "BRG" => "Jund",
        "WUBR" => "Yore",
        "WUBG" => "Witch",
        "WURG" => "Ink",
        "WBRG" => "Dune",
        "UBRG" => "Glint",
        "WUBRG" => "Five-Color",
        _ => return canonical,
    };
    named.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_and_names() {
        assert_eq!(canonical("GUW"), "WUG");
        assert_eq!(canonical("GGx"), "G");
        assert_eq!(name("GUW"), "Bant");
        assert_eq!(name(""), "Colorless");
    }
}
