use crate::core::resolve_path;
use std::path::Path;

pub fn resolve_existing_script_path(current_dir: &Path, project_dir: &Path, raw: &str) -> String {
    let p1 = resolve_path(current_dir, raw);
    if p1.exists() {
        return p1.to_string_lossy().to_string();
    }

    let p2 = resolve_path(project_dir, raw);
    if p2.exists() {
        return p2.to_string_lossy().to_string();
    }

    raw.to_string()
}

