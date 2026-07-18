use crate::agent::design_importer::{
    DesignImporter, DesignImporterDefinition, DesignImporterOutcome,
};
use crate::design_import::{
    DesignImportPreparationStage, DesignImportPublicationStage, PreparedImport,
};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, Notify};

#[derive(Clone, Debug)]
pub struct DesignImportRequest {
    pub source: PathBuf,
    pub cache_dir: PathBuf,
    pub packages_dir: PathBuf,
    pub configured_browser: Option<PathBuf>,
    pub render_timeout: Duration,
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignImportWorkflowStage {
    Validating,
    Copying,
    Extracting,
    RenderingPreviews,
    Analyzing,
    ValidatingPackage,
    Publishing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesignImportWorkflowEvent {
    Stage(DesignImportWorkflowStage),
    Completed {
        package_name: String,
        package_path: PathBuf,
    },
    Failed(String),
    Cancelled,
}

#[derive(Clone, Default)]
pub struct DesignImportWorkflow {
    active: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    importer: Arc<Mutex<Option<DesignImporter>>>,
    finished: Arc<Notify>,
}

impl DesignImportWorkflow {
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn cancel(&self) -> bool {
        if !self.is_active() {
            return false;
        }
        self.cancelled.store(true, Ordering::Release);
        if let Some(importer) = self
            .importer
            .lock()
            .expect("design import workflow mutex poisoned")
            .as_ref()
        {
            importer.cancel();
        }
        true
    }

    pub async fn shutdown(&self) {
        self.cancel();
        while self.is_active() {
            let finished = self.finished.notified();
            if !self.is_active() {
                break;
            }
            finished.await;
        }
    }

    pub fn start(
        &self,
        request: DesignImportRequest,
        events: mpsc::UnboundedSender<DesignImportWorkflowEvent>,
    ) -> Result<()> {
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            anyhow::bail!("a design import is already active");
        }
        self.cancelled.store(false, Ordering::Release);
        let workflow = self.clone();
        tokio::spawn(async move {
            workflow.run(request, events).await;
        });
        Ok(())
    }

    async fn run(
        &self,
        request: DesignImportRequest,
        events: mpsc::UnboundedSender<DesignImportWorkflowEvent>,
    ) {
        let _active_guard = ActiveGuard {
            active: self.active.clone(),
            finished: self.finished.clone(),
        };
        let prepared = PreparedImport::prepare_with_progress(
            &request.source,
            &request.cache_dir,
            request.configured_browser.as_deref(),
            request.render_timeout,
            {
                let events = events.clone();
                let cancelled = self.cancelled.clone();
                move |stage| {
                    if !cancelled.load(Ordering::Acquire) {
                        let _ =
                            events.send(DesignImportWorkflowEvent::Stage(map_preparation(stage)));
                    }
                }
            },
        )
        .await;
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.finish_error(&events, error);
                return;
            }
        };
        if self.is_cancelled() {
            prepared.discard();
            let _ = events.send(DesignImportWorkflowEvent::Cancelled);
            return;
        }

        let _ = events.send(DesignImportWorkflowEvent::Stage(
            DesignImportWorkflowStage::Analyzing,
        ));
        let importer = match DesignImporterDefinition
            .create(&request.provider, &request.model, &prepared.job_dir)
            .await
        {
            Ok(importer) => importer,
            Err(error) => {
                prepared.discard();
                self.finish_error(&events, error);
                return;
            }
        };
        *self
            .importer
            .lock()
            .expect("design import workflow mutex poisoned") = Some(importer.clone());
        if self.is_cancelled() {
            prepared.discard();
            self.clear_importer();
            let _ = events.send(DesignImportWorkflowEvent::Cancelled);
            return;
        }
        let outcome = importer
            .run(prepared.prompt(), prepared.contact_sheet.clone())
            .await;
        self.clear_importer();
        let output = match outcome {
            Ok(DesignImporterOutcome::Completed(output)) if !self.is_cancelled() => output,
            Ok(DesignImporterOutcome::Completed(_)) | Ok(DesignImporterOutcome::Cancelled) => {
                prepared.discard();
                let _ = events.send(DesignImportWorkflowEvent::Cancelled);
                return;
            }
            Err(error) => {
                prepared.discard();
                self.finish_error(&events, error);
                return;
            }
        };

        let packages_dir = request.packages_dir;
        let publish_events = events.clone();
        let cancelled = self.cancelled.clone();
        let published = tokio::task::spawn_blocking(move || {
            let result = prepared.publish_with_progress(&output, &packages_dir, |stage| {
                if !cancelled.load(Ordering::Acquire) {
                    let _ = publish_events
                        .send(DesignImportWorkflowEvent::Stage(map_publication(stage)));
                }
            });
            if result.is_err() {
                prepared.discard();
            }
            result
        })
        .await;
        match published {
            Ok(Ok(package)) if !self.is_cancelled() => {
                let _ = events.send(DesignImportWorkflowEvent::Completed {
                    package_name: package.name,
                    package_path: package.path,
                });
            }
            Ok(Ok(package)) => {
                let _ = std::fs::remove_dir_all(&package.path);
                let _ = events.send(DesignImportWorkflowEvent::Cancelled);
            }
            Ok(Err(error)) => self.finish_error(&events, error),
            Err(error) => self.finish_error(&events, error.into()),
        }
    }

    fn finish_error(
        &self,
        events: &mpsc::UnboundedSender<DesignImportWorkflowEvent>,
        error: anyhow::Error,
    ) {
        let event = if self.is_cancelled() {
            DesignImportWorkflowEvent::Cancelled
        } else {
            DesignImportWorkflowEvent::Failed(format!("{error:#}"))
        };
        let _ = events.send(event);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn clear_importer(&self) {
        *self
            .importer
            .lock()
            .expect("design import workflow mutex poisoned") = None;
    }
}

struct ActiveGuard {
    active: Arc<AtomicBool>,
    finished: Arc<Notify>,
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        self.finished.notify_waiters();
    }
}

fn map_preparation(stage: DesignImportPreparationStage) -> DesignImportWorkflowStage {
    match stage {
        DesignImportPreparationStage::Validating => DesignImportWorkflowStage::Validating,
        DesignImportPreparationStage::Copying => DesignImportWorkflowStage::Copying,
        DesignImportPreparationStage::Extracting => DesignImportWorkflowStage::Extracting,
        DesignImportPreparationStage::RenderingPreviews => {
            DesignImportWorkflowStage::RenderingPreviews
        }
    }
}

fn map_publication(stage: DesignImportPublicationStage) -> DesignImportWorkflowStage {
    match stage {
        DesignImportPublicationStage::ValidatingPackage => {
            DesignImportWorkflowStage::ValidatingPackage
        }
        DesignImportPublicationStage::Publishing => DesignImportWorkflowStage::Publishing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_mappings_are_exhaustive() {
        assert_eq!(
            map_preparation(DesignImportPreparationStage::RenderingPreviews),
            DesignImportWorkflowStage::RenderingPreviews
        );
        assert_eq!(
            map_publication(DesignImportPublicationStage::Publishing),
            DesignImportWorkflowStage::Publishing
        );
    }

    #[test]
    fn inactive_workflow_does_not_report_cancellation() {
        assert!(!DesignImportWorkflow::default().cancel());
    }
}
