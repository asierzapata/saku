use serde::{Serialize, de::DeserializeOwned};

/// Trait that every storable entity must implement.
///
/// Provides the mapping from entity fields to a deterministic storage key.
/// The storage key is `"{entity_type}/{natural_key}"`.
pub trait Entity: Serialize + DeserializeOwned {
    /// The entity type prefix (e.g., "project", "task", "area").
    fn entity_type() -> &'static str;

    /// Derive the natural key from this entity's fields.
    /// For projects/areas this is `name.to_lowercase()`.
    /// For tasks this is the pre-computed hash suffix.
    fn natural_key(&self) -> String;

    /// Full storage key: `"{entity_type}/{natural_key}"`.
    fn storage_key(&self) -> String {
        format!("{}/{}", Self::entity_type(), self.natural_key())
    }
}

/// Describes an entity type's structure for generic reference repair.
///
/// Used by `repair_references` to know which fields on which entity types
/// hold foreign-key references to other entity types.
pub struct EntitySchema {
    /// The entity type prefix (e.g., "project").
    pub entity_type: &'static str,

    /// List of (field_name, target_entity_type) pairs.
    /// For example, `("area_key", "area")` means the `area_key` field
    /// holds a reference to an entity with type prefix "area".
    pub references: Vec<(&'static str, &'static str)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct TestProject {
        name: String,
    }

    impl Entity for TestProject {
        fn entity_type() -> &'static str {
            "project"
        }

        fn natural_key(&self) -> String {
            self.name.to_lowercase()
        }
    }

    #[test]
    fn storage_key_combines_type_and_natural_key() {
        let p = TestProject {
            name: "Website".into(),
        };
        assert_eq!(p.storage_key(), "project/website");
    }

    #[test]
    fn natural_key_is_lowercase() {
        let p = TestProject {
            name: "My BLOG".into(),
        };
        assert_eq!(p.natural_key(), "my blog");
        assert_eq!(p.storage_key(), "project/my blog");
    }
}
