use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

#[derive(Debug, Clone, Copy)]
pub struct WorkLimiterConfig {
    pub library_scan_limit: u32,
    pub media_analysis_limit: u32,
    pub tmdb_metadata_limit: u32,
}

impl WorkLimiterConfig {
    pub fn normalized(self) -> Self {
        Self {
            library_scan_limit: self.library_scan_limit.max(1),
            media_analysis_limit: self.media_analysis_limit.max(1),
            tmdb_metadata_limit: self.tmdb_metadata_limit.max(1),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WorkLimiterKind {
    LibraryScan,
    MediaAnalysis,
    TmdbMetadata,
}

#[derive(Clone)]
pub struct WorkLimiters {
    inner: Arc<LimiterInner>,
}

struct LimiterInner {
    state: Mutex<LimiterState>,
    notify: Notify,
}

#[derive(Debug)]
struct LimiterState {
    limits: WorkLimiterConfig,
    library_scan_active: u32,
    media_analysis_active: u32,
    tmdb_metadata_active: u32,
}

pub struct WorkPermit {
    inner: Arc<LimiterInner>,
    kind: WorkLimiterKind,
}

impl WorkLimiters {
    pub fn new(config: WorkLimiterConfig) -> Self {
        Self {
            inner: Arc::new(LimiterInner {
                state: Mutex::new(LimiterState {
                    limits: config.normalized(),
                    library_scan_active: 0,
                    media_analysis_active: 0,
                    tmdb_metadata_active: 0,
                }),
                notify: Notify::new(),
            }),
        }
    }

    pub async fn configure(&self, config: WorkLimiterConfig) {
        let mut state = self.inner.state.lock().await;
        state.limits = config.normalized();
        drop(state);
        self.inner.notify.notify_waiters();
    }

    pub async fn acquire(&self, kind: WorkLimiterKind) -> WorkPermit {
        loop {
            let notified = {
                let mut state = self.inner.state.lock().await;
                if state.try_acquire(kind) {
                    return WorkPermit {
                        inner: self.inner.clone(),
                        kind,
                    };
                }
                self.inner.notify.notified()
            };
            notified.await;
        }
    }
}

impl LimiterState {
    fn try_acquire(&mut self, kind: WorkLimiterKind) -> bool {
        match kind {
            WorkLimiterKind::LibraryScan => {
                if self.library_scan_active >= self.limits.library_scan_limit {
                    return false;
                }
                self.library_scan_active += 1;
            }
            WorkLimiterKind::MediaAnalysis => {
                if self.media_analysis_active >= self.limits.media_analysis_limit {
                    return false;
                }
                self.media_analysis_active += 1;
            }
            WorkLimiterKind::TmdbMetadata => {
                if self.tmdb_metadata_active >= self.limits.tmdb_metadata_limit {
                    return false;
                }
                self.tmdb_metadata_active += 1;
            }
        }
        true
    }

    fn release(&mut self, kind: WorkLimiterKind) {
        match kind {
            WorkLimiterKind::LibraryScan => {
                self.library_scan_active = self.library_scan_active.saturating_sub(1);
            }
            WorkLimiterKind::MediaAnalysis => {
                self.media_analysis_active = self.media_analysis_active.saturating_sub(1);
            }
            WorkLimiterKind::TmdbMetadata => {
                self.tmdb_metadata_active = self.tmdb_metadata_active.saturating_sub(1);
            }
        }
    }
}

impl Drop for WorkPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.inner.state.try_lock() {
            state.release(self.kind);
            drop(state);
            self.inner.notify.notify_waiters();
            return;
        }

        let inner = self.inner.clone();
        let kind = self.kind;
        tokio::spawn(async move {
            let mut state = inner.state.lock().await;
            state.release(kind);
            drop(state);
            inner.notify.notify_waiters();
        });
    }
}
