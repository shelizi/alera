//! Bundled source SVGs from Material Icon Theme, also used by Flutter Classic.
//! Only a curated set is embedded; unknown types fall back to Lucide/Codicons.
//! Upstream license: ../assets/material-icon-theme/LICENSE.upstream.

/// Exact rendered SVG for common file/folder names, without runtime disk IO.
/// Closed assets retain their upstream multicolor fills (do not tint them).
/// Matching open-folder SVGs are not yet bundled: fall back to Classic's
/// Lucide folderOpen icon rather than displaying a closed folder while open.
pub fn material_icon_svg(
    path_or_name: &str,
    is_directory: bool,
    expanded: bool,
) -> Option<&'static [u8]> {
    let name = path_or_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path_or_name)
        .to_ascii_lowercase();
    if is_directory {
        // Classic's material theme supplies separate SVGs for expanded
        // folders. Until those exact vectors are bundled, use its Lucide
        // folder-open fallback instead of falsely showing a closed folder.
        if expanded {
            return None;
        }
        return Some(match name.as_str() {
            "src" | "source" => include_bytes!("../assets/material-icon-theme/folder-src.svg"),
            "theme" | "themes" => include_bytes!("../assets/material-icon-theme/folder-theme.svg"),
            _ => include_bytes!("../assets/material-icon-theme/folder-base.svg"),
        });
    }
    let svg: &'static [u8] = match name.as_str() {
        "dockerfile"
        | "docker-compose.yml"
        | "docker-compose.yaml"
        | "compose.yml"
        | "compose.yaml" => {
            include_bytes!("../assets/material-icon-theme/docker.svg")
        }
        ".gitignore" | ".gitattributes" | ".gitmodules" => {
            include_bytes!("../assets/material-icon-theme/git.svg")
        }
        "tsconfig.json" | "jsconfig.json" => {
            include_bytes!("../assets/material-icon-theme/typescript.svg")
        }
        "cargo.toml" | "cargo.lock" => include_bytes!("../assets/material-icon-theme/rust.svg"),
        _ => match name.rsplit_once('.').map(|(_, ext)| ext) {
            Some("rs") => include_bytes!("../assets/material-icon-theme/rust.svg"),
            Some("dart") => include_bytes!("../assets/material-icon-theme/dart.svg"),
            Some("js" | "cjs" | "mjs") => {
                include_bytes!("../assets/material-icon-theme/javascript.svg")
            }
            Some("ts" | "mts" | "cts") => {
                include_bytes!("../assets/material-icon-theme/typescript.svg")
            }
            Some("jsx" | "tsx") => include_bytes!("../assets/material-icon-theme/react.svg"),
            Some("py" | "pyw") => include_bytes!("../assets/material-icon-theme/python.svg"),
            Some("go") => include_bytes!("../assets/material-icon-theme/go.svg"),
            Some("java") => include_bytes!("../assets/material-icon-theme/java.svg"),
            Some("cc" | "cpp" | "cxx" | "hpp") => {
                include_bytes!("../assets/material-icon-theme/cpp.svg")
            }
            Some("php") => include_bytes!("../assets/material-icon-theme/php.svg"),
            Some("swift") => include_bytes!("../assets/material-icon-theme/swift.svg"),
            Some("kt" | "kts") => include_bytes!("../assets/material-icon-theme/kotlin.svg"),
            Some("json" | "jsonc" | "jsonl") => {
                include_bytes!("../assets/material-icon-theme/json.svg")
            }
            Some("yaml" | "yml") => include_bytes!("../assets/material-icon-theme/yaml.svg"),
            Some("xml" | "xsl") => include_bytes!("../assets/material-icon-theme/xml.svg"),
            Some("toml") => include_bytes!("../assets/material-icon-theme/toml.svg"),
            Some("htm" | "html") => include_bytes!("../assets/material-icon-theme/html.svg"),
            Some("css" | "scss" | "sass" | "less") => {
                include_bytes!("../assets/material-icon-theme/css.svg")
            }
            Some("md" | "mdx" | "markdown") => {
                include_bytes!("../assets/material-icon-theme/markdown.svg")
            }
            Some("pdf") => include_bytes!("../assets/material-icon-theme/pdf.svg"),
            Some("zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "epub") => {
                include_bytes!("../assets/material-icon-theme/zip.svg")
            }
            Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "ico" | "bmp") => {
                include_bytes!("../assets/material-icon-theme/image.svg")
            }
            Some("svg") => include_bytes!("../assets/material-icon-theme/svg.svg"),
            Some("mp4" | "mkv" | "webm" | "mov" | "avi") => {
                include_bytes!("../assets/material-icon-theme/video.svg")
            }
            Some("mp3" | "wav" | "flac" | "ogg" | "m4a") => {
                include_bytes!("../assets/material-icon-theme/audio.svg")
            }
            Some("sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd") => {
                include_bytes!("../assets/material-icon-theme/console.svg")
            }
            _ => return None,
        },
    };
    Some(svg)
}

#[cfg(test)]
mod tests {
    use super::material_icon_svg;

    #[test]
    fn common_extensions_and_named_files_use_bundled_multicolor_svgs() {
        for name in [
            "src/main.rs",
            "lib/main.dart",
            "index.tsx",
            "Dockerfile",
            "Cargo.toml",
            "README.md",
            "photo.png",
            "spec.pdf",
            "build.zip",
            "script.ps1",
            "app.json",
            "style.css",
        ] {
            let icon = material_icon_svg(name, false, false).expect(name);
            assert!(
                icon.starts_with(b"<svg") || icon.starts_with(b"<?xml"),
                "{name}"
            );
            assert!(icon.len() > 80, "{name} icon too small");
        }
        assert_ne!(
            material_icon_svg("hello.rs", false, false),
            material_icon_svg("hello.py", false, false)
        );
    }

    #[test]
    fn folders_and_windows_paths_are_supported() {
        assert!(material_icon_svg("src", true, false).is_some());
        assert!(material_icon_svg("src", true, true).is_none());
        assert!(material_icon_svg("C:\\temp\\main.dart", false, false).is_some());
        assert!(material_icon_svg("mystery.xyz", false, false).is_none());
        assert!(material_icon_svg("untitled", false, false).is_none());
    }


    #[test]
    fn expanded_folders_use_classic_open_fallback_instead_of_closed_material_art() {
        for folder in ["src", "theme", "unknown", ".github", "C:\\demo\\src"] {
            assert!(material_icon_svg(folder, true, false).is_some(), "{folder}");
            assert!(material_icon_svg(folder, true, true).is_none(), "{folder}");
        }
        // When open Material assets are unavailable, keep Classic's Lucide
        // fallback rather than rendering the CLOSED folder art as open.
        let icons = include_str!("native_ui_icons.rs");
        assert!(icons.contains("UiIcon::FolderOpen"));
        assert!(icons.contains("explorer_file_icon(name, is_directory, expanded)"));
        assert!(icons.contains("UiIcon::FolderOpen | UiIcon::Open => LucideIcon::FolderOpen"));
    }
    #[test]
    fn every_embedded_asset_is_self_contained_svg() {
        let inputs = [
            "a.rs",
            "a.dart",
            "a.js",
            "a.ts",
            "a.py",
            "a.go",
            "a.java",
            "a.cpp",
            "a.php",
            "a.swift",
            "a.kt",
            "a.json",
            "a.yaml",
            "a.xml",
            "a.toml",
            "a.html",
            "a.css",
            "a.md",
            "a.pdf",
            "a.zip",
            "a.png",
            "a.svg",
            "a.mp4",
            "a.mp3",
            "a.ps1",
            "Dockerfile",
            "a.gitignore",
            "package.json",
        ];
        for name in inputs {
            if let Some(svg) = material_icon_svg(name, false, false) {
                let data = std::str::from_utf8(svg).expect(name);
                assert!(data.contains("</svg>") || data.ends_with("/>"), "{name}");
                assert!(!data.contains("<script"), "{name}");
                assert!(!data.contains("<foreignObject"), "{name}");
            }
        }
    }
}
