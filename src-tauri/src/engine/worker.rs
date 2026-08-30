use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use duckdb::Connection;

use super::{catalog, EngineError, EngineProfile, ProjectCatalog};
use crate::{
    metadata::sources::SourceRecord,
    sources::{
        operations::{self, SourceMutationResult},
        CsvOptions, ImportOptions, SourceInspection,
    },
};

const JOB_QUEUE_CAPACITY: usize = 16;

enum Job {
    ApplyProfile {
        profile: EngineProfile,
        reply: SyncSender<Result<(), EngineError>>,
    },
    InspectCatalog {
        reply: SyncSender<Result<ProjectCatalog, EngineError>>,
    },
    InspectSource {
        path: PathBuf,
        csv: Option<CsvOptions>,
        reply: SyncSender<Result<SourceInspection, EngineError>>,
    },
    LinkParquet {
        project_id: String,
        path: PathBuf,
        view_name: String,
        reply: SyncSender<Result<SourceMutationResult, EngineError>>,
    },
    ImportTable {
        project_id: String,
        path: PathBuf,
        options: ImportOptions,
        reply: SyncSender<Result<SourceMutationResult, EngineError>>,
    },
    RepairLink {
        source: SourceRecord,
        replacement: PathBuf,
        reply: SyncSender<Result<SourceMutationResult, EngineError>>,
    },
    DropLink {
        source: SourceRecord,
        reply: SyncSender<Result<(), EngineError>>,
    },
    #[cfg(test)]
    ExecuteBatch {
        sql: String,
        reply: SyncSender<Result<(), EngineError>>,
    },
    #[cfg(test)]
    Hold {
        entered: SyncSender<()>,
        release: Receiver<()>,
    },
    Shutdown,
}

pub struct DuckDbWorker {
    sender: SyncSender<Job>,
    interrupt: Arc<Mutex<Option<Arc<duckdb::InterruptHandle>>>>,
    thread: Option<JoinHandle<()>>,
}

impl DuckDbWorker {
    pub fn start(path: PathBuf) -> Result<Self, EngineError> {
        let (sender, receiver) = mpsc::sync_channel(JOB_QUEUE_CAPACITY);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let interrupt = Arc::new(Mutex::new(None));
        let worker_interrupt = interrupt.clone();
        let thread = thread::Builder::new()
            .name("tarik-duckdb-worker".into())
            .spawn(move || run_worker(path, receiver, started_tx, worker_interrupt))
            .map_err(EngineError::Thread)?;

        started_rx
            .recv()
            .map_err(|_| EngineError::WorkerStopped)??;
        Ok(Self {
            sender,
            interrupt,
            thread: Some(thread),
        })
    }

    pub fn apply_profile(&self, profile: EngineProfile) -> Result<(), EngineError> {
        self.request(|reply| Job::ApplyProfile { profile, reply })
    }

    pub fn inspect_catalog(&self) -> Result<ProjectCatalog, EngineError> {
        self.request(|reply| Job::InspectCatalog { reply })
    }

    pub fn inspect_source(
        &self,
        path: PathBuf,
        csv: Option<CsvOptions>,
    ) -> Result<SourceInspection, EngineError> {
        self.request(|reply| Job::InspectSource { path, csv, reply })
    }

    pub fn link_parquet(
        &self,
        project_id: String,
        path: PathBuf,
        view_name: String,
    ) -> Result<SourceMutationResult, EngineError> {
        self.request(|reply| Job::LinkParquet {
            project_id,
            path,
            view_name,
            reply,
        })
    }

    pub fn import_table(
        &self,
        project_id: String,
        path: PathBuf,
        options: ImportOptions,
    ) -> Result<SourceMutationResult, EngineError> {
        self.request(|reply| Job::ImportTable {
            project_id,
            path,
            options,
            reply,
        })
    }

    pub fn repair_link(
        &self,
        source: SourceRecord,
        replacement: PathBuf,
    ) -> Result<SourceMutationResult, EngineError> {
        self.request(|reply| Job::RepairLink {
            source,
            replacement,
            reply,
        })
    }

    pub fn drop_link(&self, source: SourceRecord) -> Result<(), EngineError> {
        self.request(|reply| Job::DropLink { source, reply })
    }

    pub fn interrupt(&self) -> Result<bool, EngineError> {
        let handle = self
            .interrupt
            .lock()
            .map_err(|_| EngineError::InterruptLock)?;
        if let Some(handle) = handle.as_ref() {
            handle.interrupt();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    #[cfg(test)]
    pub fn execute_batch(&self, sql: impl Into<String>) -> Result<(), EngineError> {
        let sql = sql.into();
        self.request(|reply| Job::ExecuteBatch { sql, reply })
    }

    fn request<T>(
        &self,
        build: impl FnOnce(SyncSender<Result<T, EngineError>>) -> Job,
    ) -> Result<T, EngineError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        match self.sender.try_send(build(reply_tx)) {
            Ok(()) => reply_rx.recv().map_err(|_| EngineError::WorkerStopped)?,
            Err(TrySendError::Full(_)) => Err(EngineError::QueueFull),
            Err(TrySendError::Disconnected(_)) => Err(EngineError::WorkerStopped),
        }
    }

    pub fn shutdown(mut self) -> Result<(), EngineError> {
        let _ = self.sender.send(Job::Shutdown);
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| EngineError::WorkerPanicked)?;
        }
        Ok(())
    }
}

impl Drop for DuckDbWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(Job::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_worker(
    path: PathBuf,
    receiver: Receiver<Job>,
    started: SyncSender<Result<(), EngineError>>,
    interrupt: Arc<Mutex<Option<Arc<duckdb::InterruptHandle>>>>,
) {
    let connection = match Connection::open(&path) {
        Ok(connection) => connection,
        Err(source) => {
            let _ = started.send(Err(EngineError::Open { path, source }));
            return;
        }
    };
    if let Ok(mut slot) = interrupt.lock() {
        *slot = Some(connection.interrupt_handle());
    } else {
        let _ = started.send(Err(EngineError::InterruptLock));
        return;
    }
    if started.send(Ok(())).is_err() {
        return;
    }

    for job in receiver {
        match job {
            Job::ApplyProfile { profile, reply } => {
                let _ = reply.send(profile.apply(&connection));
            }
            Job::InspectCatalog { reply } => {
                let _ = reply.send(catalog::inspect(&connection));
            }
            Job::InspectSource { path, csv, reply } => {
                let _ = reply.send(
                    crate::sources::inspect(&path, csv.as_ref()).map_err(EngineError::Source),
                );
            }
            Job::LinkParquet {
                project_id,
                path,
                view_name,
                reply,
            } => {
                let _ = reply.send(
                    operations::link_parquet(&connection, &project_id, &path, &view_name)
                        .map_err(EngineError::Source),
                );
            }
            Job::ImportTable {
                project_id,
                path,
                options,
                reply,
            } => {
                let _ = reply.send(
                    operations::import_table(&connection, &project_id, &path, &options)
                        .map_err(EngineError::Source),
                );
            }
            Job::RepairLink {
                source,
                replacement,
                reply,
            } => {
                let _ = reply.send(
                    operations::repair_link(&connection, &source, &replacement)
                        .map_err(EngineError::Source),
                );
            }
            Job::DropLink { source, reply } => {
                let _ = reply
                    .send(operations::drop_link(&connection, &source).map_err(EngineError::Source));
            }
            #[cfg(test)]
            Job::ExecuteBatch { sql, reply } => {
                let _ = reply.send(connection.execute_batch(&sql).map_err(EngineError::DuckDb));
            }
            #[cfg(test)]
            Job::Hold { entered, release } => {
                let _ = entered.send(());
                let _ = release.recv();
            }
            Job::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temporary_database(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tarik-{name}-{stamp}.duckdb"))
    }

    #[test]
    fn bounded_queue_rejects_excess_work() {
        let path = temporary_database("queue");
        let worker = DuckDbWorker::start(path.clone()).unwrap();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        worker
            .sender
            .send(Job::Hold {
                entered: entered_tx,
                release: release_rx,
            })
            .unwrap();
        entered_rx.recv().unwrap();

        let mut replies = Vec::new();
        for _ in 0..JOB_QUEUE_CAPACITY {
            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            worker
                .sender
                .try_send(Job::InspectCatalog { reply: reply_tx })
                .unwrap();
            replies.push(reply_rx);
        }
        let (reply_tx, _) = mpsc::sync_channel(1);
        assert!(matches!(
            worker
                .sender
                .try_send(Job::InspectCatalog { reply: reply_tx }),
            Err(TrySendError::Full(_))
        ));

        release_tx.send(()).unwrap();
        for reply in replies {
            reply.recv().unwrap().unwrap();
        }
        worker.shutdown().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn worker_serializes_connection_access_and_shuts_down() {
        let path = temporary_database("worker");
        let worker = DuckDbWorker::start(path.clone()).unwrap();
        worker
            .execute_batch("CREATE TABLE events(id INTEGER); INSERT INTO events VALUES (1);")
            .unwrap();
        let catalog = worker.inspect_catalog().unwrap();
        assert!(catalog.objects.iter().any(|object| object.name == "events"));
        worker.shutdown().unwrap();

        let connection = Connection::open(&path).unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let _ = std::fs::remove_file(path);
    }
}
