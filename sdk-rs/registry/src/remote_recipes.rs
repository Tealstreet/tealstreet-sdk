//! P8 recipe lookup: the compiled venues' versioned remote recipes, by exact id.
use safe_cex_engine::signing_remote::RemoteRecipe;

/// Every recipe this build can execute, grouped by enabled venue feature.
static GROUPS: &[&[&dyn RemoteRecipe]] = &[
    #[cfg(feature = "asterdex")]
    safe_cex_venue_asterdex::sign::REMOTE_RECIPES,
    #[cfg(feature = "coinbase")]
    safe_cex_venue_coinbase::REMOTE_RECIPES,
    #[cfg(feature = "orderly")]
    safe_cex_venue_orderly::REMOTE_RECIPES,
    #[cfg(feature = "paradex")]
    safe_cex_venue_paradex::REMOTE_RECIPES,
];

pub fn remote_recipes() -> impl Iterator<Item = &'static dyn RemoteRecipe> {
    GROUPS.iter().flat_map(|group| group.iter().copied())
}

/// The recipe family an account's venue may execute: the id prefix before the
/// first '.'. Accounts never sign another family's recipes.
pub fn recipe_family(venue: &str) -> Option<&'static str> {
    Some(match venue {
        "asterdex" => "asterdex",
        "coinbase" | "coinbasespot" => "coinbase",
        "orderly" | "tealstreet-dex" | "kodiakfi" | "mode" | "salsadex" | "what" | "woofi" => {
            "orderly"
        }
        "paradex" => "paradex",
        _ => return None,
    })
}

/// Exact versioned match only; a skewed or unknown id is never approximated.
pub fn remote_recipe(id: &str) -> Option<&'static dyn RemoteRecipe> {
    remote_recipes().find(|recipe| recipe.id() == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_lookup_is_exact() {
        let ids: Vec<_> = remote_recipes().map(|recipe| recipe.id()).collect();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(ids.len(), unique.len(), "{ids:?}");
        for id in &ids {
            assert_eq!(remote_recipe(id).map(|recipe| recipe.id()), Some(*id));
            let (family, version) = id.rsplit_once('.').unwrap();
            assert!(version.parse::<u32>().is_ok(), "{id}");
            assert!(
                remote_recipe(&format!("{family}.{}", version.parse::<u32>().unwrap() + 1))
                    .is_none()
            );
            assert!(remote_recipe(family).is_none());
        }
        #[cfg(all(
            feature = "asterdex",
            feature = "coinbase",
            feature = "orderly",
            feature = "paradex"
        ))]
        assert_eq!(
            ids,
            [
                "asterdex.v3.1",
                "asterdex.code.1",
                "coinbase.restJwtEs256.1",
                "coinbase.restJwtEd25519.1",
                "orderly.rest.1",
                "paradex.order.1",
            ]
        );
    }
}
