use std::path::{Path, PathBuf};

/// The `knap` skill's `SKILL.md`, embedded at compile time so it stays in
/// sync with the running binary regardless of how it was installed.
const SKILL_MD: &str = include_str!("../../skill/knap/SKILL.md");

/// Resolves the directory `SKILL.md` should be written into.
///
/// For `--global`, this is `~/.claude/skills/knap`. For `--path`, this is
/// the absolute form of the given path, resolving relative paths against
/// the current working directory.
///
/// # Errors
///
/// Returns an error if `global` is set and the home directory cannot be
/// determined, or if the current working directory cannot be read while
/// resolving a relative `path`.
fn resolve_target_dir(global: bool, path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if global {
        let home = dirs::home_dir().ok_or_else(|| {
            anyhow::anyhow!(
                "could not determine home directory; use --path <dir> instead of --global"
            )
        })?;
        return Ok(home.join(".claude").join("skills").join("knap"));
    }

    let path = path.expect("clap enforces exactly one of --global or --path");
    if path.is_absolute() {
        Ok(path)
    } else {
        let cwd = std::env::current_dir()?;
        Ok(cwd.join(path))
    }
}

/// Install or update the shipped `SKILL.md` at the given target.
///
/// Writes into `~/.claude/skills/knap` for `--global`, or `<path>` for
/// `--path`, creating parent directories as needed. If a `SKILL.md` already
/// exists at the target and its contents already match, no write occurs.
///
/// # Errors
///
/// Returns an error if the target directory cannot be created, or if the
/// existing or new `SKILL.md` cannot be read or written.
pub fn run(global: bool, path: Option<PathBuf>) -> anyhow::Result<()> {
    let target_dir = resolve_target_dir(global, path)?;
    std::fs::create_dir_all(&target_dir)?;

    let target_file = target_dir.join("SKILL.md");
    let status = write_if_different(&target_file, SKILL_MD)?;

    println!("{status} {}", target_file.display());
    Ok(())
}

/// The outcome of a write-if-different operation, used to report to the
/// user what happened.
enum WriteStatus {
    Installed,
    Updated,
    UpToDate,
}

impl std::fmt::Display for WriteStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WriteStatus::Installed => "installed",
            WriteStatus::Updated => "updated",
            WriteStatus::UpToDate => "already up to date:",
        };
        f.write_str(s)
    }
}

/// Writes `content` to `target_file` only if it doesn't already exist there
/// with matching bytes.
fn write_if_different(target_file: &Path, content: &str) -> anyhow::Result<WriteStatus> {
    match std::fs::read_to_string(target_file) {
        Ok(existing) if existing == content => Ok(WriteStatus::UpToDate),
        Ok(_) => {
            std::fs::write(target_file, content)?;
            Ok(WriteStatus::Updated)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::write(target_file, content)?;
            Ok(WriteStatus::Installed)
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_skill_md_into_new_target_dir() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("nested").join("dir");

        run(false, Some(target.clone())).unwrap();

        let written = std::fs::read_to_string(target.join("SKILL.md")).unwrap();
        assert_eq!(written, SKILL_MD);
    }

    #[test]
    fn rerun_with_unchanged_content_is_a_no_op() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("dir");

        run(false, Some(target.clone())).unwrap();
        let skill_md_path = target.join("SKILL.md");
        let mtime_before = std::fs::metadata(&skill_md_path)
            .unwrap()
            .modified()
            .unwrap();

        // Ensure any mtime change would be detectable.
        std::thread::sleep(std::time::Duration::from_millis(10));

        run(false, Some(target.clone())).unwrap();
        let mtime_after = std::fs::metadata(&skill_md_path)
            .unwrap()
            .modified()
            .unwrap();

        assert_eq!(mtime_before, mtime_after);
        let written = std::fs::read_to_string(&skill_md_path).unwrap();
        assert_eq!(written, SKILL_MD);
    }

    #[test]
    fn rerun_over_a_stale_copy_overwrites_it() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("dir");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("SKILL.md"), "stale content").unwrap();

        run(false, Some(target.clone())).unwrap();

        let written = std::fs::read_to_string(target.join("SKILL.md")).unwrap();
        assert_eq!(written, SKILL_MD);
    }

    #[test]
    fn relative_path_resolves_against_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let result = run(false, Some(PathBuf::from("relative/dir")));

        std::env::set_current_dir(original_cwd).unwrap();
        result.unwrap();

        let written =
            std::fs::read_to_string(temp.path().join("relative").join("dir").join("SKILL.md"))
                .unwrap();
        assert_eq!(written, SKILL_MD);
    }
}
