use std::path::Path;

/// Emit `_INDEX.md` stubs for every numbered child dir of `corpus_root` that
/// does not already contain one.  Returns the count of stubs written.
pub fn emit_index_stubs(corpus_root: &Path, engine: &str, today: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(corpus_root) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|ft| ft.is_dir()) {
            continue;
        }
        let raw = entry.file_name();
        let name = raw.to_string_lossy();
        if !is_numbered_dir(&name) {
            continue;
        }
        let index_path = entry.path().join("_INDEX.md");
        if index_path.exists() {
            continue;
        }
        let content = generate_stub(&entry.path(), engine, today);
        if std::fs::write(&index_path, content).is_ok() {
            count += 1;
        }
    }
    count
}

fn is_numbered_dir(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 3 && b[0].is_ascii_digit() && b[1].is_ascii_digit() && b[2] == b'-'
}

fn generate_stub(dir: &Path, engine: &str, today: &str) -> String {
    let dir_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let category = dir_name.get(3..).unwrap_or(dir_name);
    let rows = toc_rows(dir);
    let toc = rows.join("\n");
    format!(
        "---\ntitle: \"{dir_name}\"\ntype: index\nengine: \"{engine}\"\ncategory: \"{category}\"\nstatus: active\nschema_version: 1\ncreated: {today}\nupdated: {today}\n---\n\n<!-- TODO: write 1-3 sentence summary of this folder -->\n\n## Contents\n\n| File | Description |\n|------|-------------|\n{toc}\n"
    )
}

fn toc_rows(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| {
            e.file_type().is_ok_and(|ft| ft.is_file())
                && e.file_name().to_string_lossy().ends_with(".md")
                && e.file_name().to_string_lossy() != "_INDEX.md"
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names.iter().map(|n| format!("| [{n}](./{n}) | |")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_corpus() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn numbered_dir_without_index_gets_stub() {
        let dir = make_corpus();
        let numbered = dir.path().join("01-identity");
        std::fs::create_dir_all(&numbered).unwrap();
        std::fs::write(numbered.join("overview.md"), "").unwrap();

        let count = emit_index_stubs(dir.path(), "my-engine", "2026-05-02");

        assert_eq!(count, 1);
        let index = numbered.join("_INDEX.md");
        assert!(index.exists(), "_INDEX.md should be created");
        let content = std::fs::read_to_string(&index).unwrap();
        assert!(content.contains("title: \"01-identity\""));
        assert!(content.contains("engine: \"my-engine\""));
        assert!(content.contains("category: \"identity\""));
        assert!(content.contains("status: active"));
        assert!(content.contains("created: 2026-05-02"));
        assert!(content.contains("<!-- TODO: write 1-3 sentence summary"));
        assert!(content.contains("| [overview.md](./overview.md) | |"));
    }

    #[test]
    fn numbered_dir_with_existing_index_is_not_overwritten() {
        let dir = make_corpus();
        let numbered = dir.path().join("02-design");
        std::fs::create_dir_all(&numbered).unwrap();
        let index_path = numbered.join("_INDEX.md");
        std::fs::write(&index_path, "# existing").unwrap();

        let count = emit_index_stubs(dir.path(), "eng", "2026-05-02");

        assert_eq!(count, 0);
        let content = std::fs::read_to_string(&index_path).unwrap();
        assert_eq!(content, "# existing", "_INDEX.md must not be overwritten");
    }

    #[test]
    fn non_numbered_dir_is_ignored() {
        let dir = make_corpus();
        std::fs::create_dir_all(dir.path().join("archive")).unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();

        let count = emit_index_stubs(dir.path(), "eng", "2026-05-02");

        assert_eq!(count, 0);
        assert!(!dir.path().join("archive/_INDEX.md").exists());
        assert!(!dir.path().join("src/_INDEX.md").exists());
    }

    #[test]
    fn multiple_dirs_returns_correct_count() {
        let dir = make_corpus();
        for name in &["01-identity", "11-architecture", "31-evals"] {
            std::fs::create_dir_all(dir.path().join(name)).unwrap();
        }
        // 41-gaps already has an _INDEX.md
        let gaps = dir.path().join("41-gaps");
        std::fs::create_dir_all(&gaps).unwrap();
        std::fs::write(gaps.join("_INDEX.md"), "").unwrap();

        let count = emit_index_stubs(dir.path(), "eng", "2026-05-02");

        assert_eq!(count, 3);
    }
}
