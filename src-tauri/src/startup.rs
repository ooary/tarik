use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use serde::Serialize;

use crate::{
    observability::{AppLogger, EventFields, LogLevel},
    storage::CleanupService,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatus {
    pub ready: bool,
    pub phase: &'static str,
}

pub struct StartupCoordinator {
    ready: AtomicBool,
}

impl StartupCoordinator {
    pub fn begin(
        cleanup: Arc<CleanupService>,
        logger: Arc<AppLogger>,
    ) -> Result<Arc<Self>, std::io::Error> {
        let coordinator = Arc::new(Self {
            ready: AtomicBool::new(false),
        });
        let worker = coordinator.clone();
        std::thread::Builder::new()
            .name("tarik-startup-cleanup".into())
            .spawn(move || {
                let summary = cleanup.startup_cleanup();
                logger.record(
                    if summary.warnings.is_empty() {
                        LogLevel::Info
                    } else {
                        LogLevel::Warning
                    },
                    "storage",
                    "startup_cleanup",
                    EventFields {
                        status: Some(if summary.warnings.is_empty() {
                            "succeeded"
                        } else {
                            "completed_with_warnings"
                        }),
                        message: summary.warnings.first().map(String::as_str),
                        ..EventFields::default()
                    },
                );
                worker.ready.store(true, Ordering::Release);
            })?;
        Ok(coordinator)
    }

    fn status(&self) -> StartupStatus {
        let ready = self.ready.load(Ordering::Acquire);
        StartupStatus {
            ready,
            phase: if ready {
                "ready"
            } else {
                "checking_local_storage"
            },
        }
    }
}

#[tauri::command]
pub fn get_startup_status(coordinator: tauri::State<'_, Arc<StartupCoordinator>>) -> StartupStatus {
    coordinator.status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_bounded_cleanup_until_the_worker_finishes() {
        let coordinator = StartupCoordinator {
            ready: AtomicBool::new(false),
        };
        assert_eq!(coordinator.status().phase, "checking_local_storage");
        assert!(!coordinator.status().ready);
        coordinator.ready.store(true, Ordering::Release);
        assert_eq!(coordinator.status().phase, "ready");
        assert!(coordinator.status().ready);
    }
}
