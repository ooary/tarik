use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

use duckdb::Connection;

use super::{catalog, EngineError, EngineProfile, ProjectCatalog};

const JOB_QUEUE_CAPACITY: usize = 16;

enum Job {
    ApplyProfile {
        profile: EngineProfile,
        reply: SyncSender<Result<(), EngineError>>,
    },
    InspectCatalog {
        reply: SyncSender<Result<ProjectCatalog, EngineError>>,
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
    thread: Option<JoinHandle<()>>,
}

impl DuckDbWorker {
    pub fn start(path: PathBuf) -> Result<Self, EngineError> {
        let (sender, receiver) = mpsc::sync_channel(JOB_QUEUE_CAPACITY);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("tarik-duckdb-worker".into())
            .spawn(move || run_worker(path, receiver, started_tx))
            .map_err(EngineError::Thread)?;

        started_rx
            .recv()
            .map_err(|_| EngineError::WorkerStopped)??;
        Ok(Self {
            sender,
            thread: Some(thread),
        })
    }

    pub fn apply_profile(&self, profile: EngineProfile) -> Result<(), EngineError> {
        self.request(|reply| Job::ApplyProfile { profile, reply })
    }

    pub fn inspect_catalog(&self) -> Result<ProjectCatalog, EngineError> {
        self.request(|reply| Job::InspectCatalog { reply })
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
) {
    let connection = match Connection::open(&path) {
        Ok(connection) => connection,
        Err(source) => {
            let _ = started.send(Err(EngineError::Open { path, source }));
            return;
        }
    };
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
