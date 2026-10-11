use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(super) fn portable_config_path(environment: &BTreeMap<String, String>) -> Option<PathBuf> {
    if let Some(root) = environment.get("DEVIN_PORTABLE_ROOT") {
        if let Some(config) = config_at_root(Path::new(root.trim())) {
            return Some(config);
        }
    }
    let extensions = environment
        .get("PATHEXT")
        .map(String::as_str)
        .unwrap_or(".COM;.EXE;.BAT;.CMD");
    for directory in environment.get("PATH")?.split(';') {
        let directory = directory.trim().trim_matches('"');
        if directory.is_empty() {
            continue;
        }
        for extension in extensions.split(';').filter(|value| !value.is_empty()) {
            let launcher = Path::new(directory).join(format!("devin{}", extension.to_lowercase()));
            if launcher.is_file() {
                return config_from_launcher(&launcher);
            }
        }
    }
    None
}

fn config_at_root(root: &Path) -> Option<PathBuf> {
    if root.as_os_str().is_empty()
        || !root.join("bin/_devin-env.cmd").is_file()
        || !root.join("bin/devin.exe").is_file()
    {
        return None;
    }
    Some(root.join("data/home/AppData/Roaming/devin/config.json"))
}

fn launcher_source(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > 65536 {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn config_from_launcher(launcher: &Path) -> Option<PathBuf> {
    let extension = launcher.extension()?.to_str()?;
    if !extension.eq_ignore_ascii_case("cmd") && !extension.eq_ignore_ascii_case("bat") {
        return None;
    }
    let mut launcher = launcher.to_path_buf();
    let mut source = launcher_source(&launcher)?;
    let target = source.lines().find_map(|line| {
        let prefix = line.get(..4)?;
        let suffix = line.get(4..)?;
        (prefix.eq_ignore_ascii_case("@rem") && suffix.starts_with([' ', '\t']))
            .then(|| suffix.trim().to_string())
    });
    if let Some(target) = target {
        launcher = PathBuf::from(target);
        source = launcher_source(&launcher)?;
    }
    if !source.to_lowercase().contains("_devin-env.cmd") {
        return None;
    }
    config_at_root(launcher.parent()?.parent()?)
}

#[cfg(test)]
#[path = "integration_config_devin_paths_tests.rs"]
mod tests;
