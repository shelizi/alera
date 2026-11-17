//! Bundled source SVGs from Material Icon Theme, also used by Flutter Classic.
//! Only a curated set is embedded; unknown types fall back to Lucide/Codicons.
//! Upstream license: ../assets/material-icon-theme/LICENSE.upstream.

/// Exact rendered SVG for common file/folder names, without runtime disk IO.
/// Paired open/closed folder assets retain their upstream multicolor fills.
/// All imagery is compiled into the binary; icons are not downloaded at runtime.
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
        // Classic uses Material Icon Theme for both collapsed and expanded
        // folders; keep paired SVGs from the same pinned upstream revision.
        // Match its dot/underscore/hyphen-prefixed directory associations.
        let folder_name = name.trim_start_matches(&['.', '_', '-'][..]);
        let (closed, opened): (&'static [u8], &'static [u8]) = match folder_name {
            "src" | "srcs" | "source" | "sources" | "code" => (
                include_bytes!("../assets/material-icon-theme/folder-src.svg"),
                include_bytes!("../assets/material-icon-theme/folder-src-open.svg"),
            ),
            "theme" | "themes" => (
                include_bytes!("../assets/material-icon-theme/folder-theme.svg"),
                include_bytes!("../assets/material-icon-theme/folder-theme-open.svg"),
            ),
            "doc" | "docs" | "document" | "documents" | "documentation" | "wiki" | "news" | "blog" => (
                include_bytes!("../assets/material-icon-theme/folder-docs.svg"),
                include_bytes!("../assets/material-icon-theme/folder-docs-open.svg"),
            ),
            "test" | "tests" | "testing" | "spec" | "specs" | "testfiles" | "snapshots" => (
                include_bytes!("../assets/material-icon-theme/folder-test.svg"),
                include_bytes!("../assets/material-icon-theme/folder-test-open.svg"),
            ),
            "script" | "scripts" | "scripting" | "xtask" => (
                include_bytes!("../assets/material-icon-theme/folder-scripts.svg"),
                include_bytes!("../assets/material-icon-theme/folder-scripts-open.svg"),
            ),
            "node" | "nodejs" | "node_modules" => (
                include_bytes!("../assets/material-icon-theme/folder-node.svg"),
                include_bytes!("../assets/material-icon-theme/folder-node-open.svg"),
            ),
            "github" => (
                include_bytes!("../assets/material-icon-theme/folder-github.svg"),
                include_bytes!("../assets/material-icon-theme/folder-github-open.svg"),
            ),
            "vscode" | "vscode-test" => (
                include_bytes!("../assets/material-icon-theme/folder-vscode.svg"),
                include_bytes!("../assets/material-icon-theme/folder-vscode-open.svg"),
            ),
            "dist" | "out" | "output" | "outputs" | "build" | "builds" | "release" | "bin" | "distribution" | "compiled" => (
                include_bytes!("../assets/material-icon-theme/folder-dist.svg"),
                include_bytes!("../assets/material-icon-theme/folder-dist-open.svg"),
            ),
            "cfg" | "cfgs" | "conf" | "confs" | "config" | "configs" | "configuration" | "configurations" | "setting" | "settings" => (
                include_bytes!("../assets/material-icon-theme/folder-config.svg"),
                include_bytes!("../assets/material-icon-theme/folder-config-open.svg"),
            ),
            "component" | "components" => (
                include_bytes!("../assets/material-icon-theme/folder-components.svg"),
                include_bytes!("../assets/material-icon-theme/folder-components-open.svg"),
            ),
            "img" | "imgs" | "image" | "images" | "icons" | "icon" | "pics" | "pictures" | "photos" | "screenshots" => (
                include_bytes!("../assets/material-icon-theme/folder-images.svg"),
                include_bytes!("../assets/material-icon-theme/folder-images-open.svg"),
            ),
            "app" | "apps" => (
                include_bytes!("../assets/material-icon-theme/folder-app.svg"),
                include_bytes!("../assets/material-icon-theme/folder-app-open.svg"),
            ),
            _ => (
                include_bytes!("../assets/material-icon-theme/folder-base.svg"),
                include_bytes!("../assets/material-icon-theme/folder-base-open.svg"),
            ),
        };
        return Some(if expanded { opened } else { closed });
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
        assert!(material_icon_svg("src", true, true).is_some());
        assert!(material_icon_svg("C:\\temp\\main.dart", false, false).is_some());
        assert!(material_icon_svg("mystery.xyz", false, false).is_none());
        assert!(material_icon_svg("untitled", false, false).is_none());
    }


    #[test]
    fn expanded_folders_use_classic_multicolor_material_open_art() {
        // Exact opened-folder geometry from the pinned upstream
        // folderGenerator.ts, retaining each closed icon's colored motive.
        const UPSTREAM_OPEN_PATH: &str = "M14.483 6H4.721a1 1 0 0 0-.949.684L2 12V5h12a1 1 0 0 0-1-1H7.562a1 1 0 0 1-.64-.232l-.644-.536A1 1 0 0 0 5.638 3H2a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h11l2.403-5.606A1 1 0 0 0 14.483 6";
        for folder in [
            "src", "theme", "docs", "test", "scripts", "node_modules",
            ".github", ".vscode", "dist", "config", "components",
            "images", "app", "unknown", "C:\\demo\\src",
        ] {
            let closed = material_icon_svg(folder, true, false).expect(folder);
            let opened = material_icon_svg(folder, true, true).expect(folder);
            assert_ne!(opened, closed, "{folder}: opened icon must differ from closed");
            let svg = std::str::from_utf8(opened).expect("SVG must be UTF-8");
            assert!(svg.contains(UPSTREAM_OPEN_PATH), "{folder}: wrong open geometry");
            assert!(svg.contains("id=\"motive\""), "{folder}: lost colored motive");
            assert!(!svg.contains("<script") && !svg.contains("<foreignObject"));
        }
        assert_ne!(material_icon_svg(".github", true, false),
                   material_icon_svg("unknown", true, false));
        assert_ne!(material_icon_svg("src", true, true),
                   material_icon_svg("theme", true, true));
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
