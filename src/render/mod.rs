//! Generation-safe rendering service.
//!
//! The service serializes expensive renders, debounces queued mutations, and
//! only emits results for the newest requested generation. A stale render is
//! allowed to finish so cache writes remain transactional, but its result is
//! never presented to the UI.

pub mod browser;
pub mod cache;
pub mod pipeline;

use anyhow::Result;
use cache::RenderManifest;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub type RenderFuture = Pin<Box<dyn Future<Output = Result<RenderProduct>> + Send + 'static>>;

#[derive(Clone, Debug)]
pub struct RenderRequest {
    /// The committed DeckEngine generation captured under its deck lock.
    pub generation: u64,
    pub deck_identity: Vec<u8>,
    pub cache_key: cache::CacheKey,
    pub html: Arc<str>,
    pub slide_count: u32,
}

#[derive(Clone, Debug)]
pub struct RenderProduct {
    pub directory: PathBuf,
    pub manifest: RenderManifest,
}

#[derive(Clone, Debug)]
pub enum RenderEvent {
    Started {
        generation: u64,
    },
    Done {
        generation: u64,
        product: RenderProduct,
    },
    Failed {
        generation: u64,
        error: String,
    },
}

/// Object-safe abstraction used by the service and straightforward to fake in
/// tests without `async_trait`.
pub trait RenderBackend: Send + Sync + 'static {
    fn render(&self, request: RenderRequest) -> RenderFuture;
}

impl RenderBackend for pipeline::BrowserPipeline {
    fn render(&self, request: RenderRequest) -> RenderFuture {
        let pipeline = self.clone();
        Box::pin(async move {
            let (directory, manifest) = pipeline
                .render(
                    request.generation,
                    &request.deck_identity,
                    request.cache_key,
                    &request.html,
                    request.slide_count,
                )
                .await?;
            Ok(RenderProduct {
                directory,
                manifest,
            })
        })
    }
}

pub struct RenderService {
    request_tx: mpsc::UnboundedSender<RenderRequest>,
    events: Mutex<Option<mpsc::UnboundedReceiver<RenderEvent>>>,
    latest_generation: Arc<AtomicU64>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl RenderService {
    pub fn new(backend: Arc<dyn RenderBackend>, debounce: Duration) -> Self {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let latest_generation = Arc::new(AtomicU64::new(0));
        let worker = tokio::spawn(worker(
            request_rx,
            event_tx,
            backend,
            debounce,
            latest_generation.clone(),
        ));
        Self {
            request_tx,
            events: Mutex::new(Some(event_rx)),
            latest_generation,
            worker: Mutex::new(Some(worker)),
        }
    }

    /// Queue a render. Requests older than the latest observed generation are
    /// rejected. Requests for the same generation are permitted for Ctrl+R.
    pub fn request(&self, request: RenderRequest) -> Result<bool> {
        let generation = request.generation;
        let mut current = self.latest_generation.load(Ordering::Acquire);
        loop {
            if generation < current {
                return Ok(false);
            }
            match self.latest_generation.compare_exchange_weak(
                current,
                generation,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
        self.request_tx
            .send(request)
            .map_err(|_| anyhow::anyhow!("render service has stopped"))?;
        Ok(true)
    }

    pub fn latest_generation(&self) -> u64 {
        self.latest_generation.load(Ordering::Acquire)
    }

    /// Stop pending and in-flight browser work before its cache directory is removed.
    pub async fn shutdown(&self) {
        let worker = self
            .worker
            .lock()
            .expect("render worker mutex poisoned")
            .take();
        if let Some(worker) = worker {
            worker.abort();
            let _ = worker.await;
        }
    }

    /// Event streams have one consumer, normally the app event pump.
    pub fn take_events(&self) -> Option<mpsc::UnboundedReceiver<RenderEvent>> {
        self.events
            .lock()
            .expect("render event mutex poisoned")
            .take()
    }
}

async fn worker(
    mut requests: mpsc::UnboundedReceiver<RenderRequest>,
    events: mpsc::UnboundedSender<RenderEvent>,
    backend: Arc<dyn RenderBackend>,
    debounce: Duration,
    latest: Arc<AtomicU64>,
) {
    let mut pending: Option<RenderRequest> = None;
    let mut input_closed = false;
    loop {
        if pending.is_none() {
            if input_closed {
                break;
            }
            pending = requests.recv().await;
            if pending.is_none() {
                break;
            }
        }
        pending = debounce_latest(pending.take().unwrap(), &mut requests, debounce).await;
        let request = pending.take().expect("debounce always returns a request");
        let generation = request.generation;
        if generation < latest.load(Ordering::Acquire) {
            continue;
        }
        let _ = events.send(RenderEvent::Started { generation });
        let render = backend.render(request);
        tokio::pin!(render);
        loop {
            tokio::select! {
                result = &mut render => {
                    if latest.load(Ordering::Acquire) == generation {
                        let event = match result {
                            Ok(product) if product.manifest.generation == generation => RenderEvent::Done { generation, product },
                            Ok(product) => RenderEvent::Failed { generation, error: format!("backend returned manifest generation {} for request {generation}", product.manifest.generation) },
                            Err(error) => RenderEvent::Failed { generation, error: format!("{error:#}") },
                        };
                        let _ = events.send(event);
                    }
                    break;
                }
                next = requests.recv(), if !input_closed => {
                    match next {
                        Some(request) => {
                            if pending.as_ref().map_or(true, |old| request.generation >= old.generation) {
                                pending = Some(request);
                            }
                            while let Ok(request) = requests.try_recv() {
                                if pending.as_ref().map_or(true, |old| request.generation >= old.generation) {
                                    pending = Some(request);
                                }
                            }
                        }
                        None => input_closed = true,
                    }
                }
            }
        }
    }
}

async fn debounce_latest(
    mut latest_request: RenderRequest,
    requests: &mut mpsc::UnboundedReceiver<RenderRequest>,
    debounce: Duration,
) -> Option<RenderRequest> {
    if debounce.is_zero() {
        while let Ok(request) = requests.try_recv() {
            if request.generation >= latest_request.generation {
                latest_request = request;
            }
        }
        return Some(latest_request);
    }
    let delay = tokio::time::sleep(debounce);
    tokio::pin!(delay);
    loop {
        tokio::select! {
            _ = &mut delay => return Some(latest_request),
            request = requests.recv() => match request {
                Some(request) => {
                    if request.generation >= latest_request.generation { latest_request = request; }
                    // Debounce from the most recent mutation, not the first one.
                    delay.as_mut().reset(tokio::time::Instant::now() + debounce);
                }
                None => return Some(latest_request),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cache::{CacheKey, RenderManifest};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        calls: Arc<AtomicUsize>,
        delay: Duration,
    }
    impl RenderBackend for Fake {
        fn render(&self, request: RenderRequest) -> RenderFuture {
            let calls = self.calls.clone();
            let delay = self.delay;
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(delay).await;
                let manifest = RenderManifest::new(request.generation, request.cache_key, vec![])?;
                Ok(RenderProduct {
                    directory: "/tmp/render-test".into(),
                    manifest,
                })
            })
        }
    }
    fn request(generation: u64) -> RenderRequest {
        RenderRequest {
            generation,
            deck_identity: vec![1],
            cache_key: CacheKey::new(b"d", "h", "r", 1, 1, 1.0).unwrap(),
            html: "<html/>".into(),
            slide_count: 1,
        }
    }

    #[tokio::test]
    async fn debounce_and_generation_safety_publish_only_latest() {
        let calls = Arc::new(AtomicUsize::new(0));
        let service = RenderService::new(
            Arc::new(Fake {
                calls: calls.clone(),
                delay: Duration::from_millis(25),
            }),
            Duration::from_millis(10),
        );
        let mut events = service.take_events().unwrap();
        service.request(request(1)).unwrap();
        service.request(request(2)).unwrap();
        loop {
            if let Some(RenderEvent::Done { generation, .. }) = events.recv().await {
                assert_eq!(generation, 2);
                break;
            }
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(!service.request(request(1)).unwrap());
    }

    #[tokio::test]
    async fn stale_in_flight_result_is_discarded() {
        let service = RenderService::new(
            Arc::new(Fake {
                calls: Arc::new(AtomicUsize::new(0)),
                delay: Duration::from_millis(30),
            }),
            Duration::ZERO,
        );
        let mut events = service.take_events().unwrap();
        service.request(request(1)).unwrap();
        while !matches!(
            events.recv().await,
            Some(RenderEvent::Started { generation: 1 })
        ) {}
        service.request(request(2)).unwrap();
        loop {
            match events.recv().await {
                Some(RenderEvent::Done { generation: 2, .. }) => break,
                Some(
                    RenderEvent::Done { generation, .. } | RenderEvent::Failed { generation, .. },
                ) if generation == 1 => panic!("stale event published"),
                _ => {}
            }
        }
    }
}
