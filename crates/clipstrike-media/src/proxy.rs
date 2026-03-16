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
                // Here we would use ffmpeg-next or gstreamer to actually
                // transcode the file to a lower resolution proxy.
                // For demonstration, we simulate the processing time.
                std::thread::sleep(std::time::Duration::from_millis(100));

                let output_path = std::env::temp_dir().join(format!("proxy_{}.mp4", req.id));

                // Pretend generation succeeded
                let result = ProxyResult {
                    id: req.id,
                    proxy_path: Ok(output_path),
                };

                // Send result back to async context
                let _ = sender.blocking_send(result);
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_proxy_generator_processes_batch() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(10);
        let generator = ProxyGenerator::new(sender);

        let requests = vec![
            ProxyRequest {
                id: 1,
                source_path: PathBuf::from("video1.mp4"),
                resolution_scale: 0.25,
            },
            ProxyRequest {
                id: 2,
                source_path: PathBuf::from("video2.mp4"),
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

        assert_eq!(results.len(), 2, "Should receive exact number of requested proxies");
        
        // Results arrive in non-deterministic order due to Rayon par_iter
        let mut ids: Vec<u64> = results.iter().map(|r| r.id).collect();
        ids.sort();
        
        assert_eq!(ids, vec![1, 2]);
        assert!(results.iter().all(|r| r.proxy_path.is_ok()));
    }
}
