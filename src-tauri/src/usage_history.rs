use crate::models::UsageSample;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const FORMAT_VERSION: i32 = 1;
const MARKER_NAME: &str = ".codex-limits-history.json";
const INSTALLATIONS_DIR: &str = "installations";
const RETENTION_SECS: i64 = 90 * 86_400;
const MAX_FILE_SIZE: u64 = 1_000_000;

#[derive(Serialize, Deserialize)]
struct Marker {
    version: i32,
}

#[derive(Serialize, Deserialize)]
struct DailyFile {
    version: i32,
    samples: Vec<UsageSample>,
}

pub struct HistoryState {
    pub samples: Vec<UsageSample>,
    pub folder_name: Option<String>,
    pub error_message: Option<String>,
}

pub struct UsageHistory {
    local_directory: PathBuf,
    installation_id: String,
    sync_directory: Option<PathBuf>,
    error_message: Option<String>,
}

impl UsageHistory {
    pub fn new(local_directory: PathBuf, installation_id: String) -> Self {
        Self {
            local_directory,
            installation_id,
            sync_directory: None,
            error_message: None,
        }
    }

    pub fn load(&mut self) -> HistoryState {
        match self.prepare_root(&self.local_directory.clone(), true) {
            Ok(()) => {
                let _ = self.remove_expired_own_files(&self.local_directory.clone());
                self.error_message = None;
            }
            Err(_) => {
                self.error_message = Some("Usage history couldn't be saved.".to_string());
            }
        }
        self.state()
    }

    pub fn record(&mut self, sample: UsageSample) -> HistoryState {
        let local = self.local_directory.clone();
        match self.prepare_root(&local, true) {
            Ok(()) => {
                let _ = self.remove_expired_own_files(&local);
                if let Err(_) = self.add_samples(&[sample.clone()], &local, &self.installation_id.clone()) {
                    self.error_message = Some("Usage history couldn't be saved.".to_string());
                } else if let Some(sync) = self.sync_directory.clone() {
                    if self.prepare_root(&sync, false).is_ok() {
                        let _ = self.remove_expired_own_files(&sync);
                        let _ = self.add_samples(&[sample], &sync, &self.installation_id.clone());
                    }
                }
                if self.error_message.is_none() {
                    self.error_message = None;
                }
            }
            Err(_) => {
                self.error_message = Some("Usage history couldn't be saved.".to_string());
            }
        }
        self.state()
    }

    pub fn connect(&mut self, directory: PathBuf) -> HistoryState {
        if self.prepare_root(&directory, false).is_err() {
            self.error_message = Some("Choose an empty folder or an existing Codex Limits history folder.".to_string());
            return self.state();
        }

        let local = self.local_directory.clone();
        let existing = self.read_all(&local);
        let _ = self.add_samples(&existing, &local, &self.installation_id.clone());
        self.sync_directory = Some(directory);
        self.error_message = None;
        self.synchronize()
    }

    pub fn disconnect(&mut self) -> HistoryState {
        self.sync_directory = None;
        self.error_message = None;
        self.state()
    }

    pub fn synchronize(&mut self) -> HistoryState {
        let sync = match self.sync_directory.clone() {
            Some(s) => s,
            None => return self.state(),
        };

        if self.prepare_root(&sync, false).is_err() {
            self.error_message = Some("Sync paused — folder unavailable.".to_string());
            return self.state();
        }

        let local = self.local_directory.clone();
        let _ = self.remove_expired_own_files(&local);
        let _ = self.remove_expired_own_files(&sync);

        let had_import_errors = self.import_history(&sync);
        let _ = self.publish_own_history(&sync);

        self.error_message = if had_import_errors {
            Some("Some synced history couldn't be read.".to_string())
        } else {
            None
        };

        self.state()
    }

    pub fn sync_folder_name(&self) -> Option<String> {
        self.sync_directory
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
    }

    fn state(&self) -> HistoryState {
        let local = self.local_directory.clone();
        let samples = self.read_all(&local);
        HistoryState {
            samples,
            folder_name: self.sync_folder_name(),
            error_message: self.error_message.clone(),
        }
    }

    fn prepare_root(&self, root: &Path, create_if_missing: bool) -> Result<(), ()> {
        if !root.exists() {
            if !create_if_missing {
                return Err(());
            }
            std::fs::create_dir_all(root).map_err(|_| ())?;
        } else if !root.is_dir() {
            return Err(());
        }

        let marker_path = root.join(MARKER_NAME);
        if marker_path.exists() {
            let data = std::fs::read_to_string(&marker_path).map_err(|_| ())?;
            let marker: Marker = serde_json::from_str(&data).map_err(|_| ())?;
            if marker.version != FORMAT_VERSION {
                return Err(());
            }
        } else {
            let entries: Vec<_> = std::fs::read_dir(root)
                .map_err(|_| ())?
                .filter_map(|e| e.ok())
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .collect();
            if !entries.is_empty() {
                return Err(());
            }
            let marker = Marker { version: FORMAT_VERSION };
            let json = serde_json::to_string(&marker).map_err(|_| ())?;
            std::fs::write(&marker_path, json).map_err(|_| ())?;
        }

        let installations = root.join(INSTALLATIONS_DIR);
        std::fs::create_dir_all(&installations).map_err(|_| ())?;
        Ok(())
    }

    fn add_samples(&self, samples: &[UsageSample], root: &Path, installation_id: &str) -> Result<(), ()> {
        let valid = normalize(samples);
        if valid.is_empty() {
            return Ok(());
        }

        let writer_dir = root.join(INSTALLATIONS_DIR).join(installation_id);
        std::fs::create_dir_all(&writer_dir).map_err(|_| ())?;

        let mut grouped: std::collections::HashMap<String, Vec<UsageSample>> = std::collections::HashMap::new();
        for s in &valid {
            let day = day_name(s.observed_at);
            grouped.entry(day).or_default().push(s.clone());
        }

        for (day, new_samples) in grouped {
            let file_path = writer_dir.join(format!("{}.json", day));
            let existing = read_daily_file(&file_path).unwrap_or_default();
            let merged = normalize(&[existing, new_samples].concat());
            write_daily_file(&file_path, &merged)?;
        }

        Ok(())
    }

    fn import_history(&self, remote_root: &Path) -> bool {
        let remote_installations = remote_root.join(INSTALLATIONS_DIR);
        let local_installations = self.local_directory.join(INSTALLATIONS_DIR);
        let mut had_error = false;

        let writers = match std::fs::read_dir(&remote_installations) {
            Ok(rd) => rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).collect::<Vec<_>>(),
            Err(_) => return false,
        };

        for writer in writers {
            let local_writer = local_installations.join(writer.file_name());
            let _ = std::fs::create_dir_all(&local_writer);

            let files = match std::fs::read_dir(writer.path()) {
                Ok(rd) => rd.filter_map(|e| e.ok()).filter(|e| {
                    e.path().extension().map(|ext| ext == "json").unwrap_or(false)
                }).collect::<Vec<_>>(),
                Err(_) => continue,
            };

            for file in files {
                let local_file = local_writer.join(file.file_name());
                match (read_daily_file(&file.path()), read_daily_file(&local_file)) {
                    (Ok(remote_samples), Ok(local_samples)) => {
                        let merged = normalize(&[local_samples, remote_samples].concat());
                        if write_daily_file(&local_file, &merged).is_err() {
                            had_error = true;
                        }
                    }
                    _ => had_error = true,
                }
            }
        }

        had_error
    }

    fn publish_own_history(&self, remote_root: &Path) -> Result<(), ()> {
        let local_writer = self.local_directory.join(INSTALLATIONS_DIR).join(&self.installation_id);
        if !local_writer.exists() {
            return Ok(());
        }

        let remote_writer = remote_root.join(INSTALLATIONS_DIR).join(&self.installation_id);
        std::fs::create_dir_all(&remote_writer).map_err(|_| ())?;

        let files: Vec<_> = std::fs::read_dir(&local_writer)
            .map_err(|_| ())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "json").unwrap_or(false))
            .collect();

        for file in files {
            let remote_file = remote_writer.join(file.file_name());
            let local_samples = read_daily_file(&file.path()).unwrap_or_default();
            let remote_samples = read_daily_file(&remote_file).unwrap_or_default();
            let merged = normalize(&[local_samples, remote_samples].concat());
            write_daily_file(&file.path(), &merged)?;
            write_daily_file(&remote_file, &merged)?;
        }

        Ok(())
    }

    fn remove_expired_own_files(&self, root: &Path) -> Result<(), ()> {
        let writer = root.join(INSTALLATIONS_DIR).join(&self.installation_id);
        if !writer.exists() {
            return Ok(());
        }

        let files: Vec<_> = std::fs::read_dir(&writer)
            .map_err(|_| ())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "json").unwrap_or(false))
            .collect();

        for file in files {
            let path = file.path();
            match read_daily_file(&path) {
                Ok(existing) => {
                    let retained = normalize(&existing);
                    if retained.is_empty() {
                        let _ = std::fs::remove_file(&path);
                    } else if retained.len() != existing.len() {
                        let _ = write_daily_file(&path, &retained);
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(())
    }

    fn read_all(&self, root: &Path) -> Vec<UsageSample> {
        let installations = root.join(INSTALLATIONS_DIR);
        let mut samples = Vec::new();

        let writers = match std::fs::read_dir(&installations) {
            Ok(rd) => rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).collect::<Vec<_>>(),
            Err(_) => return samples,
        };

        for writer in writers {
            let files = match std::fs::read_dir(writer.path()) {
                Ok(rd) => rd.filter_map(|e| e.ok()).filter(|e| {
                    e.path().extension().map(|ext| ext == "json").unwrap_or(false)
                }).collect::<Vec<_>>(),
                Err(_) => continue,
            };

            for file in files {
                if let Ok(file_samples) = read_daily_file(&file.path()) {
                    samples.extend(file_samples);
                }
            }
        }

        normalize(&samples)
    }
}

fn read_daily_file(path: &Path) -> Result<Vec<UsageSample>, ()> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let metadata = std::fs::metadata(path).map_err(|_| ())?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(());
    }
    let data = std::fs::read_to_string(path).map_err(|_| ())?;
    let file: DailyFile = serde_json::from_str(&data).map_err(|_| ())?;
    if file.version != FORMAT_VERSION {
        return Err(());
    }
    Ok(file.samples)
}

fn write_daily_file(path: &Path, samples: &[UsageSample]) -> Result<(), ()> {
    let file = DailyFile {
        version: FORMAT_VERSION,
        samples: samples.to_vec(),
    };
    let json = serde_json::to_string(&file).map_err(|_| ())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &json).map_err(|_| ())?;
    std::fs::rename(&tmp, path).map_err(|_| ())?;
    Ok(())
}

fn normalize(samples: &[UsageSample]) -> Vec<UsageSample> {
    let now = chrono::Utc::now().timestamp();
    let cutoff = now - RETENTION_SECS;

    let mut seen = HashSet::new();
    let mut result: Vec<UsageSample> = samples
        .iter()
        .filter(|s| {
            s.observed_at >= cutoff
                && s.observed_at <= s.resets_at
                && s.remaining_percent <= 100
        })
        .filter(|s| seen.insert((s.observed_at, s.remaining_percent, s.resets_at)))
        .cloned()
        .collect();

    result.sort_by(|a, b| {
        a.observed_at.cmp(&b.observed_at)
            .then(b.remaining_percent.cmp(&a.remaining_percent))
            .then(a.resets_at.cmp(&b.resets_at))
    });

    result
}

fn day_name(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}
