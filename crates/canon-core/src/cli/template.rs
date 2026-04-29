use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::template::manifest::FolderShape;
use crate::template::validate::validate as validate_template;
use crate::template::{
    production_builtins, ListedTemplate, LoadedTemplate, TemplateError, TemplateLoader,
    TemplateTier,
};

// ---------------------------------------------------------------------------
// Git subprocess trait boundary (Rule 1 — SPAWN-SAFE: behind trait GitCloner)
// ---------------------------------------------------------------------------

pub trait GitCloner {
    fn clone_repo(&self, url: &str, dest: &Path) -> Result<(), String>;
}

pub struct DefaultGitCloner;

impl GitCloner for DefaultGitCloner {
    fn clone_repo(&self, url: &str, dest: &Path) -> Result<(), String> {
        // SPAWN-SAFE: behind trait GitCloner
        let status = std::process::Command::new("git")
            .args(["clone", "--", url, &dest.to_string_lossy()])
            .status()
            .map_err(|e| format!("failed to spawn git: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("git clone failed (exit: {status})"))
        }
    }
}

// ---------------------------------------------------------------------------
// Production entry point
// ---------------------------------------------------------------------------

pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let workspace_root = find_workspace_root();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let user_dir = default_user_templates_dir();
    run_impl(
        args,
        out,
        err,
        &DefaultGitCloner,
        &workspace_root,
        &cwd,
        &user_dir,
    )
}

/// Testable implementation — accepts injected paths per Rule 13.
pub fn run_impl(
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
    git: &dyn GitCloner,
    workspace_root: &Path,
    cwd: &Path,
    user_dir: &Path,
) -> i32 {
    let Some(subcmd) = args.first() else {
        let _ = writeln!(
            err,
            "canon template: no subcommand. Try 'canon template list'."
        );
        print_help(err);
        return 2;
    };
    match subcmd.as_str() {
        "list" => cmd_list(&args[1..], out, err, workspace_root, user_dir),
        "show" => cmd_show(&args[1..], out, err, workspace_root),
        "validate" => cmd_validate(&args[1..], out, err, workspace_root, cwd),
        "install" => cmd_install(&args[1..], out, err, git, cwd, user_dir),
        "--help" | "-h" | "help" => {
            print_help(out);
            0
        }
        other => {
            let _ = writeln!(
                err,
                "canon template: unknown subcommand '{}'. Try 'canon template list'.",
                other
            );
            2
        }
    }
}

// ---------------------------------------------------------------------------
// canon template list
// ---------------------------------------------------------------------------

fn cmd_list(
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
    workspace_root: &Path,
    user_dir: &Path,
) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        let _ = writeln!(
            out,
            "USAGE:\n  canon template list\n\nLists all installed templates organized by tier (built-in, workspace, user)."
        );
        return 0;
    }
    if let Some(unknown) = args.iter().find(|a| a.starts_with('-')) {
        let _ = writeln!(err, "error: unknown flag '{}'", unknown);
        return 2;
    }

    let loader = TemplateLoader::with_builtins(
        production_builtins(),
        workspace_root.join(".accelmars/canon/templates"),
        user_dir.to_owned(),
    );

    let all = loader.list_all();
    format_list(&all, user_dir, out);
    0
}

pub fn format_list(templates: &[ListedTemplate], user_dir: &Path, out: &mut dyn Write) {
    let builtins: Vec<_> = templates
        .iter()
        .filter(|t| t.tier == TemplateTier::BuiltIn)
        .collect();
    let workspace_ts: Vec<_> = templates
        .iter()
        .filter(|t| t.tier == TemplateTier::Workspace)
        .collect();
    let user_ts: Vec<_> = templates
        .iter()
        .filter(|t| t.tier == TemplateTier::User)
        .collect();

    let _ = writeln!(out, "BUILT-IN TEMPLATES");
    if builtins.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for t in &builtins {
            let _ = writeln!(out, "  {:<22}  {}", t.name, t.description);
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "WORKSPACE TEMPLATES (.accelmars/canon/templates/)");
    if workspace_ts.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for t in &workspace_ts {
            let _ = writeln!(out, "  {:<22}  {}", t.name, t.description);
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "USER TEMPLATES ({}/)", user_dir.display());
    if user_ts.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for t in &user_ts {
            let _ = writeln!(out, "  {:<22}  {}", t.name, t.description);
        }
    }
}

// ---------------------------------------------------------------------------
// canon template show
// ---------------------------------------------------------------------------

fn cmd_show(
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
    workspace_root: &Path,
) -> i32 {
    let mut name_or_path: Option<String> = None;
    let mut use_json = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--format" | "-f" => {
                i += 1;
                if i >= args.len() {
                    let _ = writeln!(err, "error: --format requires a value");
                    return 2;
                }
                match args[i].as_str() {
                    "json" => use_json = true,
                    "table" => {}
                    other => {
                        let _ = writeln!(
                            err,
                            "error: unknown format '{}'; valid values: table, json",
                            other
                        );
                        return 2;
                    }
                }
            }
            "--help" | "-h" => {
                let _ = writeln!(
                    out,
                    "USAGE:\n  canon template show <name> [--format table|json]\n\nPrints manifest details for the named template."
                );
                return 0;
            }
            arg if !arg.starts_with('-') => {
                if name_or_path.is_some() {
                    let _ = writeln!(
                        err,
                        "error: unexpected argument '{}' (name already set)",
                        arg
                    );
                    return 2;
                }
                name_or_path = Some(arg.to_string());
            }
            other => {
                let _ = writeln!(err, "error: unknown flag '{}'", other);
                return 2;
            }
        }
        i += 1;
    }

    let Some(spec) = name_or_path else {
        let _ = writeln!(err, "error: <name> is required");
        return 2;
    };

    let loader = TemplateLoader::from_workspace_root(workspace_root);
    let loaded = if is_path_spec(&spec) {
        loader.load_by_path(Path::new(&spec))
    } else {
        loader.load_by_name(&spec)
    };

    match loaded {
        Ok(t) => {
            if use_json {
                show_json(&t, out);
            } else {
                show_table(&t, out);
            }
            0
        }
        Err(e) => {
            let _ = writeln!(err, "error: {}", e);
            2
        }
    }
}

fn show_table(t: &LoadedTemplate, out: &mut dyn Write) {
    let m = &t.manifest;
    let tier_str = tier_display_name(&t.tier);
    let _ = writeln!(out, "name:             {}", m.name);
    let _ = writeln!(out, "version:          {}", m.version);
    let _ = writeln!(out, "description:      {}", m.description);
    let _ = writeln!(out, "tier:             {}", tier_str);
    let _ = writeln!(
        out,
        "shape:            {}",
        shape_str(&m.folder_rules.shape)
    );
    if let Some(ref fm) = m.frontmatter {
        let _ = writeln!(out, "schema:           {}", fm.schema);
    }
    if let Some(ref inv) = m.invariants {
        let _ = writeln!(out, "index_required:   {}", inv.index_required);
        let _ = writeln!(out, "atomic_file_gate: {}", inv.atomic_file_gate);
        if let Some(ref gf) = inv.gaps_folder {
            let _ = writeln!(out, "gaps_folder:      {}", gf);
        }
    }
    if let Some(ref dir) = t.dir {
        let _ = writeln!(out, "path:             {}", dir.display());
    }
}

fn show_json(t: &LoadedTemplate, out: &mut dyn Write) {
    let m = &t.manifest;
    let tier_str = tier_display_name(&t.tier);

    let mut obj = serde_json::Map::new();
    obj.insert("name".into(), json!(m.name));
    obj.insert("version".into(), json!(m.version));
    obj.insert("description".into(), json!(m.description));
    obj.insert("tier".into(), json!(tier_str));
    obj.insert(
        "folder_rules".into(),
        json!({ "shape": shape_str(&m.folder_rules.shape) }),
    );
    if let Some(ref fm) = m.frontmatter {
        obj.insert("frontmatter".into(), json!({ "schema": fm.schema }));
    }
    if let Some(ref inv) = m.invariants {
        let mut inv_map = serde_json::Map::new();
        inv_map.insert("index_required".into(), json!(inv.index_required));
        inv_map.insert("atomic_file_gate".into(), json!(inv.atomic_file_gate));
        if let Some(ref gf) = inv.gaps_folder {
            inv_map.insert("gaps_folder".into(), json!(gf));
        }
        obj.insert("invariants".into(), serde_json::Value::Object(inv_map));
    }
    if let Some(ref dir) = t.dir {
        obj.insert("path".into(), json!(dir.display().to_string()));
    }

    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(&serde_json::Value::Object(obj))
            .unwrap_or_else(|_| "{}".to_string())
    );
}

// ---------------------------------------------------------------------------
// canon template validate
// ---------------------------------------------------------------------------

fn cmd_validate(
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
    workspace_root: &Path,
    cwd: &Path,
) -> i32 {
    let mut path_str: Option<String> = None;

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => {
                let _ = writeln!(
                    out,
                    "USAGE:\n  canon template validate <path>\n\n\
                     Validates a template directory.\n\n\
                     EXIT CODES:\n  0  Valid\n  1  Invalid (with field/line info)\n  2  Error (unreadable)"
                );
                return 0;
            }
            a if !a.starts_with('-') => {
                if path_str.is_some() {
                    let _ = writeln!(err, "error: unexpected argument '{}' (path already set)", a);
                    return 2;
                }
                path_str = Some(a.to_string());
            }
            other => {
                let _ = writeln!(err, "error: unknown flag '{}'", other);
                return 2;
            }
        }
    }

    let Some(ps) = path_str else {
        let _ = writeln!(err, "error: <path> is required");
        return 2;
    };

    // Rule 13: src path resolution (template dir must exist)
    let template_path = {
        let p = Path::new(&ps);
        if p.is_absolute() {
            p.to_owned()
        } else {
            let ws_rel = workspace_root.join(&ps);
            if ws_rel.exists() {
                ws_rel
            } else {
                let cwd_rel = cwd.join(&ps);
                if cwd_rel.exists() {
                    cwd_rel
                } else {
                    let _ = writeln!(
                        err,
                        "error: template path '{}' not found\nHint: paths are resolved from workspace root ({})",
                        ps,
                        workspace_root.display()
                    );
                    return 2;
                }
            }
        }
    };

    let loader = TemplateLoader::from_workspace_root(workspace_root);
    match loader.load_by_path(&template_path) {
        Err(TemplateError::Malformed { source, error }) => {
            let _ = writeln!(err, "invalid: {} — {}", source, error);
            1
        }
        Err(e) => {
            let _ = writeln!(err, "error: {}", e);
            2
        }
        Ok(t) => match validate_template(&t) {
            Err(TemplateError::MissingSchema {
                template,
                schema_path,
            }) => {
                let _ = writeln!(
                    err,
                    "invalid: template '{}' — schema '{}' not found",
                    template,
                    schema_path.display()
                );
                1
            }
            Err(TemplateError::Malformed { source, error }) => {
                let _ = writeln!(err, "invalid: {} — {}", source, error);
                1
            }
            Err(e) => {
                let _ = writeln!(err, "error: {}", e);
                2
            }
            Ok(()) => {
                let _ = writeln!(
                    out,
                    "valid: template '{}' at '{}'",
                    t.manifest.name,
                    template_path.display()
                );
                0
            }
        },
    }
}

// ---------------------------------------------------------------------------
// canon template install
// ---------------------------------------------------------------------------

fn cmd_install(
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
    git: &dyn GitCloner,
    cwd: &Path,
    user_dir: &Path,
) -> i32 {
    let mut source: Option<String> = None;

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => {
                let _ = writeln!(
                    out,
                    "USAGE:\n  canon template install <local-path|git-url>\n\n\
                     Installs a template to the user templates directory.\n\
                     For local paths: copies the template directory.\n\
                     For git URLs: clones the repository.\n\n\
                     SECURITY NOTE: v1 install is unsigned and user-local.\n\
                     Registry, signing, and trust model are out of scope for v1."
                );
                return 0;
            }
            a if !a.starts_with('-') => {
                if source.is_some() {
                    let _ = writeln!(err, "error: unexpected argument '{}'", a);
                    return 2;
                }
                source = Some(a.to_string());
            }
            other => {
                let _ = writeln!(err, "error: unknown flag '{}'", other);
                return 2;
            }
        }
    }

    let Some(src) = source else {
        let _ = writeln!(err, "error: <source> is required");
        return 2;
    };

    if is_git_url(&src) {
        install_from_git(&src, git, user_dir, out, err)
    } else {
        install_from_local(&src, cwd, user_dir, out, err)
    }
}

fn is_git_url(s: &str) -> bool {
    s.starts_with("https://") || s.starts_with("http://") || s.starts_with("git@")
}

fn install_from_git(
    url: &str,
    git: &dyn GitCloner,
    user_dir: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let name = url
        .rsplit('/')
        .next()
        .unwrap_or("template")
        .trim_end_matches(".git")
        .to_string();

    if !is_valid_template_name(&name) {
        let _ = writeln!(
            err,
            "error: could not derive a valid template name from URL '{}'",
            url
        );
        return 2;
    }

    if let Err(e) = fs::create_dir_all(user_dir) {
        let _ = writeln!(
            err,
            "error: could not create user templates directory: {}",
            e
        );
        return 2;
    }

    let dest = user_dir.join(&name);
    if dest.exists() {
        let _ = writeln!(
            err,
            "error: '{}' already exists; remove it first to reinstall",
            dest.display()
        );
        return 2;
    }

    if let Err(e) = git.clone_repo(url, &dest) {
        let _ = writeln!(err, "error: {}", e);
        return 2;
    }

    let _ = writeln!(out, "installed: '{}' → '{}'", name, dest.display());
    0
}

fn install_from_local(
    path_str: &str,
    cwd: &Path,
    user_dir: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let p = Path::new(path_str);
    let src = if p.is_absolute() {
        p.to_owned()
    } else {
        let cwd_rel = cwd.join(path_str);
        if cwd_rel.exists() {
            cwd_rel
        } else {
            let _ = writeln!(err, "error: local path '{}' not found", path_str);
            return 2;
        }
    };

    if !src.is_dir() {
        let _ = writeln!(err, "error: '{}' is not a directory", src.display());
        return 2;
    }

    let manifest_path = src.join("manifest.toml");
    let content = match fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(err, "error: could not read manifest.toml: {}", e);
            return 2;
        }
    };

    let name = match get_name_from_manifest(&content) {
        Ok(n) => n,
        Err(e) => {
            let _ = writeln!(err, "error: {}", e);
            return 2;
        }
    };

    if let Err(e) = fs::create_dir_all(user_dir) {
        let _ = writeln!(
            err,
            "error: could not create user templates directory: {}",
            e
        );
        return 2;
    }

    let dest = user_dir.join(&name);
    if dest.exists() {
        let _ = writeln!(
            err,
            "error: '{}' already exists; remove it first to reinstall",
            dest.display()
        );
        return 2;
    }

    if let Err(e) = copy_dir_recursive(&src, &dest) {
        let _ = writeln!(err, "error: copy failed: {}", e);
        return 2;
    }

    let _ = writeln!(out, "installed: '{}' → '{}'", name, dest.display());
    0
}

fn get_name_from_manifest(content: &str) -> Result<String, String> {
    let val: toml::Value =
        toml::from_str(content).map_err(|e| format!("invalid manifest.toml: {e}"))?;
    val.get("name")
        .and_then(|v| v.as_str())
        .filter(|n| is_valid_template_name(n))
        .map(|n| n.to_string())
        .ok_or_else(|| "manifest.toml missing or invalid 'name' field".to_string())
}

fn is_valid_template_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Recursively copy a directory. Uses entry.file_type() per Rule 12 (no symlink following).
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        // Rule 12: file_type() does not follow symlinks — avoids infinite loop on symlink-to-ancestor.
        let ft = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else if ft.is_file() {
            fs::copy(entry.path(), &dest_path)?;
        }
        // Symlinks are intentionally skipped.
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_path_spec(spec: &str) -> bool {
    spec.starts_with('/') || spec.starts_with("./") || spec.starts_with("../")
}

fn tier_display_name(tier: &TemplateTier) -> String {
    match tier {
        TemplateTier::BuiltIn => "built-in".to_string(),
        TemplateTier::Workspace => "workspace".to_string(),
        TemplateTier::User => "user".to_string(),
        TemplateTier::ExplicitPath(p) => p.display().to_string(),
    }
}

fn shape_str(shape: &FolderShape) -> &'static str {
    match shape {
        FolderShape::NumberedTiers => "numbered-tiers",
        FolderShape::Flat => "flat",
        FolderShape::ByDomain => "by-domain",
        FolderShape::Custom => "custom",
    }
}

pub(crate) fn default_user_templates_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/canon/templates");
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("canon/templates");
    }
    PathBuf::from(".config/canon/templates")
}

fn find_workspace_root() -> PathBuf {
    if let Ok(cwd) = std::env::current_dir() {
        let mut cur: &Path = &cwd;
        loop {
            if cur.join(".accelmars").is_dir() {
                return cur.to_owned();
            }
            match cur.parent() {
                Some(p) => cur = p,
                None => break,
            }
        }
        return cwd;
    }
    PathBuf::from(".")
}

fn print_help(out: &mut dyn Write) {
    let _ = writeln!(
        out,
        "USAGE:\n  canon template <subcommand> [options]\n\n\
         SUBCOMMANDS:\n\
         \x20 list               List all installed templates\n\
         \x20 show <name>        Show manifest details for a template\n\
         \x20 validate <path>    Validate a template directory\n\
         \x20 install <source>   Install a template (local path or git URL)"
    );
}
