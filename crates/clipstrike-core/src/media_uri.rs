//! Safe, diagnostic labels for media URIs at log and error boundaries.

use std::path::Path;
use url::Url;

pub fn source_label(uri: &str) -> String {
    if let Ok(parsed) = Url::parse(uri) {
        if parsed.scheme() == "file" {
            return parsed
                .to_file_path()
                .map(|path| format!("local media {}", path.display()))
                .unwrap_or_else(|_| "local media with an invalid file URI".to_string());
        }
        return format!("{} media", parsed.scheme());
    }
    if Path::new(uri).is_absolute() {
        return format!("local media {}", uri);
    }
    "media source with an invalid URI".to_string()
}

pub fn safe_backend_detail(uri: &str, detail: &str) -> String {
    let local_file = Url::parse(uri)
        .map(|parsed| {
            parsed.scheme() == "file"
                && parsed.query().is_none()
                && parsed.fragment().is_none()
                && parsed.username().is_empty()
                && parsed.password().is_none()
        })
        .unwrap_or_else(|_| Path::new(uri).is_absolute());
    if local_file && !detail.contains("://") {
        detail.to_string()
    } else {
        "media backend reported an error (details redacted)".to_string()
    }
}

pub fn media_error(uri: &str, operation: &str, detail: &str) -> String {
    format!(
        "{} for {}: {}",
        operation,
        source_label(uri),
        safe_backend_detail(uri, detail)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_credentials_and_query_are_not_in_diagnostics() {
        let uri = "https://alice:secret@example.com/movie.mp4?token=private";
        let detail = format!("Failed to fetch {uri}");
        for output in [source_label(uri), media_error(uri, "Probe failed", &detail)] {
            for secret in ["alice", "secret", "token", "private", "example.com"] {
                assert!(!output.contains(secret), "{output}");
            }
        }
    }

    #[test]
    fn local_media_keeps_useful_path_and_safe_backend_detail() {
        let uri = "file:///C:/Media/missing.mp4";
        let output = media_error(uri, "Open failed", "Resource not found");
        assert!(output.contains("missing.mp4"), "{output}");
        assert!(output.contains("Resource not found"), "{output}");
        assert!(!media_error(uri, "Open failed", "at https://user:secret@host")
            .contains("secret"));
    }
}
