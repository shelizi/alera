use std::fs::{self, File};
use std::io::{self, Seek, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const APP_LOG_PREFIX: &str = "app";
const RUNTIME_LOG_PREFIX: &str = "runtime";
const METADATA_ENTRY_NAME: &str = "meta.json";

pub fn write_diagnostics_bundle(
    output_path: String,
    metadata_json: String,
    app_log_directory: Option<String>,
    runtime_log_directory: Option<String>,
) -> Result<(), String> {
    write_diagnostics_bundle_impl(
        Path::new(&output_path),
        &metadata_json,
        app_log_directory.as_deref(),
        runtime_log_directory.as_deref(),
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn write_diagnostics_bundle_impl(
    output_path: &Path,
    metadata_json: &str,
    app_log_directory: Option<&str>,
    runtime_log_directory: Option<&str>,
) -> Result<u64, DiagnosticsArchiveError> {
    let parent = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".alera-diagnostics-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    {
        let mut writer = ZipWriter::new(temporary.as_file_mut());
        add_log_directory(&mut writer, app_log_directory, APP_LOG_PREFIX, options)?;
        add_log_directory(
            &mut writer,
            runtime_log_directory,
            RUNTIME_LOG_PREFIX,
            options,
        )?;
        writer.start_file(METADATA_ENTRY_NAME, options)?;
        writer.write_all(metadata_json.as_bytes())?;
        writer.finish()?;
    }

    temporary.as_file_mut().flush()?;
    let bytes_written = temporary.as_file().metadata()?.len();
    if output_path.exists() {
        fs::remove_file(output_path)?;
    }
    temporary
        .persist(output_path)
        .map_err(|error| DiagnosticsArchiveError::Io(error.error))?;
    Ok(bytes_written)
}

fn add_log_directory<W: Write + Seek>(
    writer: &mut ZipWriter<W>,
    directory: Option<&str>,
    prefix: &str,
    options: SimpleFileOptions,
) -> Result<(), DiagnosticsArchiveError> {
    let Some(directory) = directory else {
        return Ok(());
    };
    let directory = Path::new(directory);
    if !directory.is_dir() {
        return Ok(());
    }

    let mut files = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "log"))
        .collect::<Vec<_>>();
    files.sort();

    for path in files {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        writer.start_file(format!("{prefix}/{file_name}"), options)?;
        let mut file = File::open(&path)?;
        io::copy(&mut file, writer)?;
    }
    Ok(())
}

#[derive(Debug)]
enum DiagnosticsArchiveError {
    Io(io::Error),
    Zip(zip::result::ZipError),
}

impl std::fmt::Display for DiagnosticsArchiveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "diagnostics archive I/O error: {error}"),
            Self::Zip(error) => write!(formatter, "diagnostics archive ZIP error: {error}"),
        }
    }
}

impl From<io::Error> for DiagnosticsArchiveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<zip::result::ZipError> for DiagnosticsArchiveError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Zip(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn diagnostics_bundle_streams_sorted_logs_and_metadata() {
        let root = tempfile::tempdir().expect("create temp dir");
        let app = root.path().join("app");
        let runtime = root.path().join("runtime");
        fs::create_dir_all(&app).expect("create app logs");
        fs::create_dir_all(&runtime).expect("create runtime logs");
        fs::write(app.join("b.log"), b"app-b").expect("write app b");
        fs::write(app.join("a.log"), b"app-a").expect("write app a");
        fs::write(app.join("notes.txt"), b"ignore").expect("write ignored file");
        fs::write(runtime.join("runtime.log"), b"runtime").expect("write runtime log");

        let output = root.path().join("bundle.zip");
        let bytes_written = write_diagnostics_bundle_impl(
            &output,
            "{\n  \"app\": {\"version\": \"test\"}\n}",
            Some(path_str(&app)),
            Some(path_str(&runtime)),
        )
        .expect("build diagnostics archive");
        assert_eq!(bytes_written, fs::metadata(&output).unwrap().len());
        let mut archive = zip::ZipArchive::new(File::open(output).unwrap()).expect("open archive");

        let names = (0..archive.len())
            .map(|index| archive.by_index(index).expect("entry").name().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["app/a.log", "app/b.log", "runtime/runtime.log", "meta.json"]
        );
        assert_eq!(read_entry(&mut archive, "app/a.log"), "app-a");
        assert_eq!(read_entry(&mut archive, "runtime/runtime.log"), "runtime");
        assert!(read_entry(&mut archive, "meta.json").contains("\"version\": \"test\""));
    }

    #[test]
    fn diagnostics_bundle_skips_missing_directories() {
        let root = tempfile::tempdir().expect("create temp dir");
        let missing = root.path().join("missing");
        let output = root.path().join("bundle.zip");
        write_diagnostics_bundle_impl(&output, "{}", Some(path_str(&missing)), None)
            .expect("build diagnostics archive");
        let mut archive = zip::ZipArchive::new(File::open(output).unwrap()).expect("open archive");
        assert_eq!(archive.len(), 1);
        assert_eq!(
            archive.by_index(0).expect("metadata").name(),
            METADATA_ENTRY_NAME
        );
    }

    #[test]
    fn diagnostics_bundle_replaces_existing_destination() {
        let root = tempfile::tempdir().expect("create temp dir");
        let output = root.path().join("bundle.zip");
        fs::write(&output, b"stale").expect("write stale output");

        write_diagnostics_bundle_impl(&output, "{}", None, None)
            .expect("replace diagnostics archive");

        let mut archive = zip::ZipArchive::new(File::open(output).unwrap()).expect("open archive");
        assert_eq!(archive.len(), 1);
        assert_eq!(
            archive.by_index(0).expect("metadata").name(),
            METADATA_ENTRY_NAME
        );
    }

    fn read_entry(archive: &mut zip::ZipArchive<File>, name: &str) -> String {
        let mut entry = archive.by_name(name).expect("named entry");
        let mut output = String::new();
        entry.read_to_string(&mut output).expect("read entry");
        output
    }

    fn path_str(path: &Path) -> &str {
        path.to_str().expect("utf-8 temp path")
    }
}
