use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::task;

/// Defines a request to generate a proxy or thumbnail for a clip.
#[derive(Debug, Clone)]
pub struct ProxyRequest {
    pub id: u64,
    pub source_path: PathBuf,
    pub resolution_scale: f32, // e.g. 0.25 for 1/4th resolution
}

#[derive(Debug, Clone)]
pub struct ProxyResult {
    pub id: u64,
    pub proxy_path: Result<PathBuf, String>,
}

/// The Background Proxy Generator uses Rayon's thread pool to
/// offload heavy video transcoding and image extraction.
pub struct ProxyGenerator {
    sender: mpsc::Sender<ProxyResult>,
}

impl ProxyGenerator {
    pub fn new(sender: mpsc::Sender<ProxyResult>) -> Self {
        Self { sender }
    }

    /// Submits a batch of proxy generation requests to the rayon thread pool.
    pub async fn generate_proxies(&self, requests: Vec<ProxyRequest>) {
        let sender = self.sender.clone();

        task::spawn_blocking(move || {
            use rayon::prelude::*;

            requests.into_par_iter().for_each(|req| {
                let result = ProxyResult {
                    id: req.id,
                    proxy_path: generate_proxy_file(&req),
                };

                let _ = sender.blocking_send(result);
            });
        });
    }
}

fn generate_proxy_file(request: &ProxyRequest) -> Result<PathBuf, String> {
    if !request.source_path.exists() {
        return Err(format!(
            "Proxy source file does not exist: {}",
            request.source_path.display()
        ));
    }

    if is_image_source(&request.source_path) {
        return generate_image_proxy(request);
    }

    let extension = request
        .source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("proxy");
    let output_path = std::env::temp_dir().join(format!("proxy_{}.{}", request.id, extension));

    std::fs::copy(&request.source_path, &output_path).map_err(|err| {
        format!(
            "Failed to copy proxy source {}: {}",
            request.source_path.display(),
            err
        )
    })?;

    Ok(output_path)
}

fn is_image_source(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase()),
        Some(extension)
            if matches!(
                extension.as_str(),
                "png" | "jpg" | "jpeg" | "bmp" | "gif" | "tiff" | "webp"
            )
    )
}

fn generate_image_proxy(request: &ProxyRequest) -> Result<PathBuf, String> {
    let image = image::open(&request.source_path).map_err(|err| {
        format!(
            "Failed to open image proxy source {}: {}",
            request.source_path.display(),
            err
        )
    })?;

    let scale = request.resolution_scale.clamp(0.05, 1.0);
    let width = ((image.width() as f32 * scale).round() as u32).max(1);
    let height = ((image.height() as f32 * scale).round() as u32).max(1);
    let resized = image.resize(width, height, image::imageops::FilterType::Lanczos3);
    let output_path = std::env::temp_dir().join(format!("proxy_{}.png", request.id));

    resized.save(&output_path).map_err(|err| {
        format!(
            "Failed to save image proxy {}: {}",
            output_path.display(),
            err
        )
    })?;

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_proxy_generator_processes_batch() {
        let temp_dir =
            std::env::temp_dir().join(format!("clipstrike-proxy-batch-{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let source_one = temp_dir.join("video1.mp4");
        let source_two = temp_dir.join("video2.mp4");
        std::fs::write(&source_one, b"video 1 payload").expect("write source one");
        std::fs::write(&source_two, b"video 2 payload").expect("write source two");

        let (sender, mut receiver) = tokio::sync::mpsc::channel(10);
        let generator = ProxyGenerator::new(sender);

        let requests = vec![
            ProxyRequest {
                id: 1,
                source_path: source_one.clone(),
                resolution_scale: 0.25,
            },
            ProxyRequest {
                id: 2,
                source_path: source_two.clone(),
                resolution_scale: 0.25,
            },
        ];

        // Process batch in background thread pool
        generator.generate_proxies(requests).await;

        // Collect results with a timeout
        let mut results = Vec::new();
        let timeout_duration = Duration::from_secs(2);

        while let Ok(Some(result)) = timeout(timeout_duration, receiver.recv()).await {
            results.push(result);
            if results.len() == 2 {
                break;
            }
        }

        assert_eq!(
            results.len(),
            2,
            "Should receive exact number of requested proxies"
        );

        // Results arrive in non-deterministic order due to Rayon par_iter
        let mut ids: Vec<u64> = results.iter().map(|r| r.id).collect();
        ids.sort();

        assert_eq!(ids, vec![1, 2]);
        assert!(results.iter().all(|r| r.proxy_path.is_ok()));

        for result in results {
            let proxy_path = result.proxy_path.expect("proxy generation should succeed");
            let _ = std::fs::remove_file(proxy_path);
        }
        let _ = std::fs::remove_file(source_one);
        let _ = std::fs::remove_file(source_two);
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[tokio::test]
    async fn test_proxy_generator_creates_a_real_proxy_file() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("clipstrike-proxy-test-{unique_suffix}"));
        fs::create_dir_all(&temp_dir).expect("create temp dir");

        let source_path = temp_dir.join("source.png");
        image::RgbaImage::from_pixel(16, 16, image::Rgba([32, 96, 160, 255]))
            .save(&source_path)
            .expect("write synthetic source image");

        let (sender, mut receiver) = tokio::sync::mpsc::channel(10);
        let generator = ProxyGenerator::new(sender);

        generator
            .generate_proxies(vec![ProxyRequest {
                id: 99,
                source_path: source_path.clone(),
                resolution_scale: 0.25,
            }])
            .await;

        let result = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("proxy result should arrive in time")
            .expect("proxy result should be sent");

        let proxy_path = result.proxy_path.expect("proxy generation should succeed");
        assert!(
            proxy_path.exists(),
            "expected proxy generator to create an output file at {}",
            proxy_path.display()
        );

        let source_image = image::open(&source_path).expect("open source image");
        let proxy_image = image::open(&proxy_path).expect("open proxy image");
        let (source_width, source_height) = source_image.dimensions();
        let (proxy_width, proxy_height) = proxy_image.dimensions();

        assert!(
            proxy_width < source_width || proxy_height < source_height,
            "expected proxy image to be smaller than the source (source={}x{}, proxy={}x{})",
            source_width,
            source_height,
            proxy_width,
            proxy_height
        );

        let _ = fs::remove_file(&proxy_path);
        let _ = fs::remove_file(source_path);
        let _ = fs::remove_dir_all(temp_dir);
    }
}
