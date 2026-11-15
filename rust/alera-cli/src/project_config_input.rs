//! Bounded stdin transport for project config updates. The Native S07 form
//! must not expose the complete ProjectConfig (including possible setup
//! commands, paths or prompts) in the OS process command-line arguments.
//! The legacy --config-json '{...}' argument stays supported for Classic CLI.
use std::io::{self, Read};

pub const MAX_PROJECT_CONFIG_JSON_BYTES: usize = 32 * 1024;

/// A lone '-' in --config-json means read an at-most-32-KiB UTF-8 JSON object
/// from stdin. This sentinel was not valid JSON in the legacy CLI, so older
/// clients remain backward compatible without new flags or an RPC version.
pub fn decode_project_config_json(argument: &str, mut input: impl Read) -> io::Result<String> {
    if argument != "-" {
        if argument.is_empty() || argument.len() > MAX_PROJECT_CONFIG_JSON_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Project config JSON exceeds 32 KiB or is empty",
            ));
        }
        return Ok(argument.to_owned());
    }
    let mut bytes = Vec::new();
    input
        .by_ref()
        .take(MAX_PROJECT_CONFIG_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > MAX_PROJECT_CONFIG_JSON_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Project config stdin is empty or exceeds 32 KiB",
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Project config stdin must be UTF-8",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn project_config_json_stdin_is_bounded_utf8_and_legacy_inline_remains_compatible() {
        let private_json = r#"{"worktree":{"setup":["export TOKEN=private"]}}"#;
        assert_eq!(
            decode_project_config_json("-", Cursor::new(private_json)).unwrap(),
            private_json
        );
        assert_eq!(
            decode_project_config_json(private_json, Cursor::new("never read")).unwrap(),
            private_json
        );
        assert!(decode_project_config_json("-", Cursor::new(Vec::<u8>::new())).is_err());
        assert!(decode_project_config_json("-", Cursor::new(vec![b'x'; 32 * 1024 + 1])).is_err());
        assert!(decode_project_config_json("-", Cursor::new(vec![0xff])).is_err());
        assert!(decode_project_config_json("", Cursor::new("ignored")).is_err());
        assert!(
            decode_project_config_json(&"x".repeat(32 * 1024 + 1), Cursor::new("ignored")).is_err()
        );
        assert_eq!(
            decode_project_config_json("-", Cursor::new(vec![b'a'; 32 * 1024]))
                .unwrap()
                .len(),
            32 * 1024
        );
    }
}
