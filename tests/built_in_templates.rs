use std::path::PathBuf;

use canon_core::template::{FolderShape, TemplateLoader, TemplateTier};

fn nonexistent_loader() -> TemplateLoader {
    TemplateLoader::with_builtins(
        canon_core::template::loader::production_builtins(),
        PathBuf::from("/nonexistent-workspace"),
        PathBuf::from("/nonexistent-user"),
    )
}

#[test]
fn canon_default_parses_without_error() {
    let loader = nonexistent_loader();
    loader
        .load_by_name("canon-default")
        .expect("canon-default built-in should parse without error");
}

#[test]
fn canon_default_manifest_matches_format_spec() {
    let loader = nonexistent_loader();
    let t = loader
        .load_by_name("canon-default")
        .expect("canon-default should load");

    assert_eq!(t.manifest.name, "canon-default");
    assert!(!t.manifest.version.is_empty(), "version must be present");
    assert!(!t.manifest.description.is_empty(), "description must be present");

    // format spec: folder_rules.shape is one of the defined shapes
    assert_eq!(t.manifest.folder_rules.shape, FolderShape::Custom,
        "canon-default uses custom shape (mixed named top-level folders)");

    // invariants: gaps_folder must be non-empty if specified
    if let Some(ref inv) = t.manifest.invariants {
        if let Some(ref gf) = inv.gaps_folder {
            assert!(!gf.is_empty(), "gaps_folder must not be empty");
        }
    }

    // built-in has no dir (compiled in)
    assert!(t.dir.is_none(), "built-in template should have no disk dir");
    assert_eq!(t.tier, TemplateTier::BuiltIn);
}

#[test]
fn canon_default_appears_in_template_list_as_builtin() {
    let loader = nonexistent_loader();
    let list = loader.list_all();

    let entry = list
        .iter()
        .find(|t| t.name == "canon-default")
        .expect("canon-default should appear in template list");

    assert_eq!(entry.tier, TemplateTier::BuiltIn);
    assert!(!entry.description.is_empty(), "listed template should have a description");
}

#[test]
fn canon_default_frontmatter_schema_is_referenced() {
    let loader = nonexistent_loader();
    let t = loader
        .load_by_name("canon-default")
        .expect("canon-default should load");

    let fm = t.manifest.frontmatter
        .expect("canon-default should reference a frontmatter schema");
    assert_eq!(fm.schema, "frontmatter.schema.json");
}

#[test]
fn builtin_templates_contain_no_accelmars_string() {
    // Verifies OSS boundary: built-in manifests must not reference AccelMars.
    let loader = nonexistent_loader();
    let list = loader.list_all();
    for entry in &list {
        if entry.tier == TemplateTier::BuiltIn {
            let t = loader.load_by_name(&entry.name).expect("should load");
            assert!(
                !t.manifest.name.contains("accelmars"),
                "built-in '{}' must not contain 'accelmars' in name",
                entry.name
            );
            assert!(
                !t.manifest.description.contains("accelmars"),
                "built-in '{}' must not contain 'accelmars' in description",
                entry.name
            );
        }
    }
}
