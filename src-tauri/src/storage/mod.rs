//! Safe cleanup for Tarik-owned result artifacts and exact abandoned export stages.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use tarik_engine_protocol::{ExportFormat, ExportOptions};

const RESULT_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const RESULT_MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_WARNINGS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CleanupSummary {
    pub artifacts_removed: u64,
    pub bytes_removed: u64,
    pub export_backups_restored: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct CleanupPolicy {
    max_age: Duration,
    max_bytes: u64,
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self {
            max_age: RESULT_MAX_AGE,
            max_bytes: RESULT_MAX_BYTES,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportCleanupManifest {
    export_id: String,
    output_directory: PathBuf,
    base_name: String,
    format: ExportFormat,
    created_at: String,
}

pub struct CleanupService {
    cache_root: PathBuf,
    result_root: PathBuf,
    export_manifest_root: PathBuf,
    policy: CleanupPolicy,
}

impl CleanupService {
    pub fn new(cache_root: PathBuf) -> Self {
        Self::with_policy(cache_root, CleanupPolicy::default())
    }

    fn with_policy(cache_root: PathBuf, policy: CleanupPolicy) -> Self {
        Self {
            result_root: cache_root.join("results"),
            export_manifest_root: cache_root.join("export-staging"),
            cache_root,
            policy,
        }
    }

    pub fn startup_cleanup(&self) -> CleanupSummary {
        let mut summary = self.cleanup_results(false);
        summary.merge(self.reconcile_export_manifests());
        summary
    }

    pub fn clear_results(&self) -> CleanupSummary {
        self.cleanup_results(true)
    }

    pub fn register_export(&self, export_id: &str, options: &ExportOptions) -> Result<(), String> {
        let manifest = manifest_from(export_id, options)?;
        fs::create_dir_all(&self.export_manifest_root).map_err(|error| {
            format!(
                "could not create export recovery directory {}: {error}",
                self.export_manifest_root.display()
            )
        })?;
        let final_path = self.manifest_path(export_id)?;
        let stage = self
            .export_manifest_root
            .join(format!(".{export_id}.json.tmp"));
        let bytes = serde_json::to_vec(&manifest)
            .map_err(|error| format!("could not encode export recovery manifest: {error}"))?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&stage)
            .map_err(|error| format!("could not create export recovery manifest: {error}"))?;
        use std::io::Write;
        if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&stage);
            return Err(format!("could not write export recovery manifest: {error}"));
        }
        if let Err(error) = fs::rename(&stage, &final_path) {
            let _ = fs::remove_file(&stage);
            return Err(format!(
                "could not publish export recovery manifest: {error}"
            ));
        }
        Ok(())
    }

    pub fn complete_export(&self, export_id: &str) {
        if let Ok(path) = self.manifest_path(export_id) {
            let _ = fs::remove_file(path);
        }
    }

    fn cleanup_results(&self, remove_all: bool) -> CleanupSummary {
        let mut summary = CleanupSummary::default();
        if !is_direct_child(&self.cache_root, &self.result_root) {
            push_warning(
                &mut summary,
                "result cache root is outside the Tarik cache directory",
            );
            return summary;
        }
        let Ok(entries) = fs::read_dir(&self.result_root) else {
            return summary;
        };
        let now = SystemTime::now();
        let mut artifacts = entries
            .filter_map(|entry| match entry {
                Ok(entry) => artifact_metadata(entry.path(), &mut summary),
                Err(error) => {
                    push_warning(
                        &mut summary,
                        format!("could not inspect result artifact: {error}"),
                    );
                    None
                }
            })
            .collect::<Vec<_>>();
        artifacts.sort_by_key(|artifact| artifact.modified);
        let mut retained_bytes = artifacts.iter().map(|artifact| artifact.bytes).sum::<u64>();
        for artifact in artifacts {
            let expired = now
                .duration_since(artifact.modified)
                .map(|age| age >= self.policy.max_age)
                .unwrap_or(false);
            let over_budget = retained_bytes > self.policy.max_bytes;
            if !(remove_all || expired || over_budget) {
                continue;
            }
            match remove_owned_artifact(&self.result_root, &artifact.path) {
                Ok(()) => {
                    summary.artifacts_removed += 1;
                    summary.bytes_removed = summary.bytes_removed.saturating_add(artifact.bytes);
                    retained_bytes = retained_bytes.saturating_sub(artifact.bytes);
                }
                Err(error) => push_warning(
                    &mut summary,
                    format!("could not remove {}: {error}", artifact.path.display()),
                ),
            }
        }
        summary
    }

    fn reconcile_export_manifests(&self) -> CleanupSummary {
        let mut summary = CleanupSummary::default();
        if !is_direct_child(&self.cache_root, &self.export_manifest_root) {
            push_warning(
                &mut summary,
                "export recovery root is outside the Tarik cache directory",
            );
            return summary;
        }
        let Ok(entries) = fs::read_dir(&self.export_manifest_root) else {
            return summary;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                push_warning(
                    &mut summary,
                    "could not inspect an export recovery manifest",
                );
                continue;
            };
            let path = entry.path();
            if !is_regular_direct_child(&self.export_manifest_root, &path)
                || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }
            let manifest = match read_manifest(&path) {
                Ok(manifest) => manifest,
                Err(error) => {
                    push_warning(&mut summary, error);
                    let _ = fs::remove_file(&path);
                    continue;
                }
            };
            reconcile_manifest(&manifest, &mut summary);
            if let Err(error) = fs::remove_file(&path) {
                push_warning(
                    &mut summary,
                    format!(
                        "could not remove recovery manifest {}: {error}",
                        path.display()
                    ),
                );
            }
        }
        summary
    }

    fn manifest_path(&self, export_id: &str) -> Result<PathBuf, String> {
        uuid::Uuid::parse_str(export_id).map_err(|_| "export ID is not a UUID".to_string())?;
        Ok(self.export_manifest_root.join(format!("{export_id}.json")))
    }
}

#[derive(Debug)]
struct Artifact {
    path: PathBuf,
    modified: SystemTime,
    bytes: u64,
}

fn artifact_metadata(path: PathBuf, summary: &mut CleanupSummary) -> Option<Artifact> {
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            push_warning(
                summary,
                format!("could not inspect {}: {error}", path.display()),
            );
            return None;
        }
    };
    if metadata.file_type().is_symlink() {
        push_warning(summary, format!("skipped symbolic link {}", path.display()));
        return None;
    }
    if !(metadata.is_file() || metadata.is_dir()) {
        push_warning(
            summary,
            format!("skipped non-file artifact {}", path.display()),
        );
        return None;
    }
    let bytes = owned_size(&path, summary);
    Some(Artifact {
        path,
        modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        bytes,
    })
}

fn owned_size(path: &Path, summary: &mut CleanupSummary) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.file_type().is_symlink() {
        return 0;
    }
    if metadata.is_file() {
        return metadata.len();
    }
    let Ok(entries) = fs::read_dir(path) else {
        push_warning(summary, format!("could not measure {}", path.display()));
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| owned_size(&entry.path(), summary))
        .fold(0u64, u64::saturating_add)
}

fn remove_owned_artifact(root: &Path, path: &Path) -> std::io::Result<()> {
    if !is_direct_child(root, path) {
        return Err(std::io::Error::other(
            "artifact is outside the owned result root",
        ));
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(std::io::Error::other("refusing to follow a symbolic link"));
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else if metadata.is_file() {
        fs::remove_file(path)
    } else {
        Err(std::io::Error::other(
            "artifact is not a regular file or directory",
        ))
    }
}

fn manifest_from(
    export_id: &str,
    options: &ExportOptions,
) -> Result<ExportCleanupManifest, String> {
    uuid::Uuid::parse_str(export_id).map_err(|_| "export ID is not a UUID".to_string())?;
    let validated = options
        .clone()
        .validate()
        .map_err(|error| format!("invalid export recovery options: {error}"))?;
    Ok(ExportCleanupManifest {
        export_id: export_id.to_string(),
        output_directory: validated.output_directory,
        base_name: validated.base_name,
        format: validated.format,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

fn read_manifest(path: &Path) -> Result<ExportCleanupManifest, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "could not read recovery manifest {}: {error}",
            path.display()
        )
    })?;
    let manifest: ExportCleanupManifest = serde_json::from_slice(&bytes)
        .map_err(|_| format!("removed malformed recovery manifest {}", path.display()))?;
    uuid::Uuid::parse_str(&manifest.export_id)
        .map_err(|_| format!("removed invalid recovery manifest {}", path.display()))?;
    let file_stem = path.file_stem().and_then(|value| value.to_str());
    if file_stem != Some(manifest.export_id.as_str())
        || !manifest.output_directory.is_absolute()
        || !manifest.output_directory.is_dir()
        || !valid_base_name(&manifest.base_name)
    {
        return Err(format!(
            "removed unsafe recovery manifest {}",
            path.display()
        ));
    }
    Ok(manifest)
}

fn reconcile_manifest(manifest: &ExportCleanupManifest, summary: &mut CleanupSummary) {
    let Ok(entries) = fs::read_dir(&manifest.output_directory) else {
        push_warning(
            summary,
            format!(
                "could not inspect abandoned export {}",
                manifest.output_directory.display()
            ),
        );
        return;
    };
    let stage_prefix = format!(".tarik-export-{}-part-", manifest.export_id);
    let backup_prefix = format!(".tarik-export-backup-{}-part-", manifest.export_id);
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        if parse_hidden_part(&name, &stage_prefix).is_some() {
            remove_recovered_file(&path, summary);
            continue;
        }
        let Some(part) = parse_hidden_part(&name, &backup_prefix) else {
            continue;
        };
        let canonical = manifest.output_directory.join(format!(
            "{}-part-{part:05}.{}",
            manifest.base_name,
            manifest.format.extension()
        ));
        if canonical.is_file() {
            remove_recovered_file(&path, summary);
        } else if !canonical.exists() {
            match fs::rename(&path, &canonical) {
                Ok(()) => summary.export_backups_restored += 1,
                Err(error) => push_warning(
                    summary,
                    format!("could not restore {}: {error}", canonical.display()),
                ),
            }
        } else {
            push_warning(
                summary,
                format!("could not restore export part over {}", canonical.display()),
            );
        }
    }
}

fn parse_hidden_part(name: &str, prefix: &str) -> Option<u64> {
    let rest = name.strip_prefix(prefix)?;
    let (part, nonce) = rest.split_once('-')?;
    if part.len() != 5
        || !part.bytes().all(|byte| byte.is_ascii_digit())
        || uuid::Uuid::parse_str(nonce.trim_end_matches(".tmp")).is_err()
    {
        return None;
    }
    part.parse().ok()
}

fn remove_recovered_file(path: &Path, summary: &mut CleanupSummary) {
    let bytes = fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    match fs::remove_file(path) {
        Ok(()) => {
            summary.artifacts_removed += 1;
            summary.bytes_removed = summary.bytes_removed.saturating_add(bytes);
        }
        Err(error) => push_warning(
            summary,
            format!("could not remove {}: {error}", path.display()),
        ),
    }
}

fn valid_base_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_direct_child(root: &Path, child: &Path) -> bool {
    child.parent() == Some(root)
}

fn is_regular_direct_child(root: &Path, path: &Path) -> bool {
    is_direct_child(root, path)
        && fs::symlink_metadata(path)
            .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
            .unwrap_or(false)
}

fn push_warning(summary: &mut CleanupSummary, warning: impl Into<String>) {
    if summary.warnings.len() < MAX_WARNINGS {
        summary.warnings.push(warning.into());
    }
}

impl CleanupSummary {
    fn merge(&mut self, other: Self) {
        self.artifacts_removed = self
            .artifacts_removed
            .saturating_add(other.artifacts_removed);
        self.bytes_removed = self.bytes_removed.saturating_add(other.bytes_removed);
        self.export_backups_restored = self
            .export_backups_restored
            .saturating_add(other.export_backups_restored);
        for warning in other.warnings {
            push_warning(self, warning);
        }
    }
}

#[tauri::command]
pub fn clear_cache(
    service: tauri::State<'_, std::sync::Arc<CleanupService>>,
    results: tauri::State<'_, std::sync::Arc<crate::results::ResultStore>>,
) -> Result<CleanupSummary, String> {
    results.release_all()?;
    Ok(service.clear_results())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarik_engine_protocol::{CsvExportOptions, ExportOverwritePolicy};

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("tarik-cleanup-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn csv_options(output: &Path) -> ExportOptions {
        ExportOptions {
            format: ExportFormat::Csv,
            output_directory: output.to_string_lossy().into_owned(),
            base_name: "orders".into(),
            rows_per_part: 10,
            overwrite: ExportOverwritePolicy::Replace,
            csv: Some(CsvExportOptions::default()),
            parquet: None,
        }
    }

    #[test]
    fn age_and_size_cleanup_preserve_fresh_artifacts_until_budget_pressure() {
        let root = temp_root("budget");
        let results = root.join("results");
        fs::create_dir_all(&results).unwrap();
        let stale = results.join("stale");
        let fresh = results.join("fresh");
        fs::create_dir_all(&stale).unwrap();
        fs::create_dir_all(&fresh).unwrap();
        fs::write(stale.join("page"), vec![0u8; 20]).unwrap();
        fs::write(fresh.join("page"), vec![0u8; 20]).unwrap();
        let old = SystemTime::now() - Duration::from_secs(60);
        fs::File::open(&stale)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
        let service = CleanupService::with_policy(
            root.clone(),
            CleanupPolicy {
                max_age: Duration::from_secs(30),
                max_bytes: 30,
            },
        );

        let summary = service.startup_cleanup();
        assert_eq!(summary.artifacts_removed, 1);
        assert!(!stale.exists());
        assert!(fresh.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn result_cleanup_never_follows_symlinks_outside_the_owned_root() {
        use std::os::unix::fs::symlink;
        let root = temp_root("symlink");
        let outside = temp_root("outside");
        let sentinel = outside.join("keep.txt");
        fs::write(&sentinel, "keep").unwrap();
        fs::create_dir_all(root.join("results")).unwrap();
        symlink(&outside, root.join("results/link")).unwrap();
        let service = CleanupService::with_policy(
            root.clone(),
            CleanupPolicy {
                max_age: Duration::ZERO,
                max_bytes: 0,
            },
        );

        let summary = service.startup_cleanup();
        assert!(sentinel.is_file());
        assert_eq!(summary.artifacts_removed, 0);
        assert!(summary
            .warnings
            .iter()
            .any(|warning| warning.contains("symbolic link")));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn export_manifest_removes_exact_stage_restores_backup_and_keeps_completed_parts() {
        let root = temp_root("export");
        let output = temp_root("output");
        let service = CleanupService::new(root.clone());
        let export_id = uuid::Uuid::new_v4().to_string();
        service
            .register_export(&export_id, &csv_options(&output))
            .unwrap();
        let stage = output.join(format!(
            ".tarik-export-{export_id}-part-00002-{}.tmp",
            uuid::Uuid::new_v4()
        ));
        let backup = output.join(format!(
            ".tarik-export-backup-{export_id}-part-00001-{}",
            uuid::Uuid::new_v4()
        ));
        let completed = output.join("orders-part-00003.csv");
        let unrelated = output.join(format!(
            ".tarik-export-{}-part-00004-{}.tmp",
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4()
        ));
        fs::write(&stage, "partial").unwrap();
        fs::write(&backup, "old complete").unwrap();
        fs::write(&completed, "completed").unwrap();
        fs::write(&unrelated, "unrelated").unwrap();

        let summary = service.startup_cleanup();
        assert!(!stage.exists());
        assert_eq!(
            fs::read_to_string(output.join("orders-part-00001.csv")).unwrap(),
            "old complete"
        );
        assert_eq!(fs::read_to_string(&completed).unwrap(), "completed");
        assert!(unrelated.exists());
        assert_eq!(summary.export_backups_restored, 1);
        assert!(!service.manifest_path(&export_id).unwrap().exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(output).unwrap();
    }

    #[test]
    fn malformed_manifest_cannot_select_an_output_directory() {
        let root = temp_root("malformed");
        let outside = temp_root("outside-safe");
        let sentinel = outside.join("keep.csv");
        fs::write(&sentinel, "keep").unwrap();
        let manifests = root.join("export-staging");
        fs::create_dir_all(&manifests).unwrap();
        fs::write(
            manifests.join(format!("{}.json", uuid::Uuid::new_v4())),
            b"{",
        )
        .unwrap();

        let service = CleanupService::new(root.clone());
        let summary = service.startup_cleanup();
        assert!(sentinel.is_file());
        assert_eq!(summary.artifacts_removed, 0);
        assert_eq!(summary.warnings.len(), 1);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
