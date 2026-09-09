use super::types::MAX_RECORD_BYTES;
use super::{LogPage, LogQuery, LogRecord, LogStore};
use async_trait::async_trait;
use chrono::{Duration, Local, NaiveDate};
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_SCAN_BYTES: usize = 256 * 1024 * 1024;
const MAX_UNIQUE_RECORDS: usize = 250_000;

#[derive(Clone)]
pub struct FileLogStore {
    inner: Arc<FileInner>,
}

struct FileInner {
    root: PathBuf,
    writer: Mutex<()>,
}

impl FileLogStore {
    pub async fn open(root: PathBuf) -> Result<Self, String> {
        tokio::task::spawn_blocking(move || {
            if !root.is_absolute()
                || root.parent().is_none()
                || root
                    .components()
                    .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
            {
                return Err("log directory must be an absolute, dedicated directory".into());
            }
            check_path(&root)?;
            fs::create_dir_all(&root).map_err(|_| "cannot create log directory")?;
            check_path(&root)?;
            let root = fs::canonicalize(root).map_err(|_| "cannot resolve log directory")?;
            let probe = tempfile::NamedTempFile::new_in(&root)
                .map_err(|_| "log directory is not writable")?;
            probe
                .as_file()
                .sync_all()
                .map_err(|_| "log directory is not writable")?;
            Ok(Self {
                inner: Arc::new(FileInner {
                    root,
                    writer: Mutex::new(()),
                }),
            })
        })
        .await
        .map_err(|_| "log directory task failed")?
    }
}

#[async_trait]
impl LogStore for FileLogStore {
    async fn append_batch(&self, records: &[LogRecord]) -> Result<(), String> {
        if records.len() > 500 {
            return Err("log batch exceeds maximum size".into());
        }
        let records = records
            .iter()
            .map(LogRecord::sanitized)
            .collect::<Result<Vec<_>, _>>()?;
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let _writer = inner.writer.lock().map_err(|_| "log writer unavailable")?;
            let mut files: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
            for record in records {
                let local = record.created_at.with_timezone(&Local);
                let directory = inner.root.join(local.format("%Y-%m-%d").to_string());
                let path = directory.join(local.format("%H.log").to_string());
                let lines = files.entry(path).or_default();
                serde_json::to_writer(&mut *lines, &record)
                    .map_err(|_| "log serialization failed")?;
                lines.push(b'\n');
            }
            for (path, lines) in files {
                let directory = path.parent().ok_or("invalid log date directory")?;
                check_path(directory)?;
                fs::create_dir_all(directory).map_err(|_| "cannot create log date directory")?;
                check_path(&path)?;
                let mut output = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .read(true)
                    .open(&path)
                    .map_err(|_| "cannot open request log")?;
                if output
                    .metadata()
                    .map_err(|_| "cannot inspect request log")?
                    .len()
                    > 0
                {
                    output
                        .seek(SeekFrom::End(-1))
                        .map_err(|_| "cannot inspect request log tail")?;
                    let mut tail = [0];
                    output
                        .read_exact(&mut tail)
                        .map_err(|_| "cannot read request log tail")?;
                    if tail[0] != b'\n' {
                        output
                            .write_all(b"\n")
                            .map_err(|_| "cannot repair request log tail")?;
                    }
                }
                output
                    .write_all(&lines)
                    .map_err(|_| "request log write failed")?;
                output.sync_data().map_err(|_| "request log sync failed")?;
            }
            Ok(())
        })
        .await
        .map_err(|_| "log writer task failed")?
    }

    async fn query(&self, query: LogQuery) -> Result<LogPage, String> {
        let query = query.normalized()?;
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let _writer = inner.writer.lock().map_err(|_| "log writer unavailable")?;
            let mut day = query.from.unwrap().with_timezone(&Local).date_naive();
            let last_day = query.to.unwrap().with_timezone(&Local).date_naive();
            let mut seen = HashSet::new();
            let mut newest: BinaryHeap<Reverse<SortedRecord>> = BinaryHeap::new();
            let keep = query.page as usize * query.page_size as usize;
            let mut total = 0;
            let mut malformed_lines = 0;
            let mut scanned = 0;
            while day <= last_day {
                for hour in 0..24 {
                    let path = inner
                        .root
                        .join(day.format("%Y-%m-%d").to_string())
                        .join(format!("{hour:02}.log"));
                    check_path(&path)?;
                    let file = match File::open(&path) {
                        Ok(file) => file,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(_) => return Err("cannot read request log".into()),
                    };
                    let mut reader = BufReader::new(file);
                    while let Some(line) = bounded_line(&mut reader, &mut scanned)? {
                        let record = line
                            .and_then(|line| serde_json::from_slice::<LogRecord>(&line).ok())
                            .and_then(|record| record.sanitized().ok());
                        let Some(record) = record else {
                            malformed_lines += 1;
                            continue;
                        };
                        if !seen.insert(record.request_id.clone()) {
                            continue;
                        }
                        if seen.len() > MAX_UNIQUE_RECORDS {
                            return Err(
                                "log query scan limit reached; narrow the time window".into()
                            );
                        }
                        if !query.matches(&record) {
                            continue;
                        }
                        total += 1;
                        newest.push(Reverse(SortedRecord(record)));
                        if newest.len() > keep {
                            newest.pop();
                        }
                    }
                }
                day = day.succ_opt().ok_or("invalid log query date")?;
            }
            let mut items: Vec<_> = newest.into_iter().map(|record| record.0).collect();
            items.sort_unstable_by(|left, right| right.cmp(left));
            let items = items
                .into_iter()
                .skip(keep - query.page_size as usize)
                .map(|record| record.0)
                .collect();
            Ok(LogPage {
                items,
                total,
                page: query.page,
                page_size: query.page_size,
                malformed_lines,
            })
        })
        .await
        .map_err(|_| "log query task failed")?
    }

    async fn retention(&self, days: Option<u32>) -> Result<u64, String> {
        let Some(days) = days else {
            return Ok(0);
        };
        if days == 0 || days > 3650 {
            return Err("invalid log retention period".into());
        }
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let _writer = inner.writer.lock().map_err(|_| "log writer unavailable")?;
            check_path(&inner.root)?;
            let cutoff = Local::now().date_naive() - Duration::days(i64::from(days));
            let mut removed = 0;
            for entry in fs::read_dir(&inner.root).map_err(|_| "cannot list log directory")? {
                let entry = entry.map_err(|_| "cannot inspect log directory")?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let Ok(day) = NaiveDate::parse_from_str(&name, "%Y-%m-%d") else {
                    continue;
                };
                if day.format("%Y-%m-%d").to_string() != name || day >= cutoff {
                    continue;
                }
                if check_path(&entry.path()).is_err() || !entry.path().is_dir() {
                    continue;
                }
                for file in
                    fs::read_dir(entry.path()).map_err(|_| "cannot list log date directory")?
                {
                    let file = file.map_err(|_| "cannot inspect log file")?;
                    let name = file.file_name().to_string_lossy().into_owned();
                    if !(0..24).any(|hour| name == format!("{hour:02}.log")) {
                        continue;
                    }
                    if check_path(&file.path()).is_err() || !file.path().is_file() {
                        continue;
                    }
                    fs::remove_file(file.path()).map_err(|_| "cannot remove expired log")?;
                    removed += 1;
                }
            }
            Ok(removed)
        })
        .await
        .map_err(|_| "log retention task failed")?
    }
}

fn check_path(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                #[cfg(windows)]
                let redirected = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let redirected = metadata.file_type().is_symlink();
                if metadata.file_type().is_symlink() || redirected {
                    return Err("linked log paths are not permitted".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("cannot validate log path".into()),
        }
    }
    Ok(())
}

fn bounded_line(
    reader: &mut impl BufRead,
    scanned: &mut usize,
) -> Result<Option<Option<Vec<u8>>>, String> {
    let mut line = Vec::new();
    let mut overflow = false;
    let mut read = 0;
    loop {
        let buffer = reader.fill_buf().map_err(|_| "cannot read request log")?;
        if buffer.is_empty() {
            return Ok(if read == 0 { None } else { Some(None) });
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(buffer.len(), |index| index + 1);
        *scanned += count;
        if *scanned > MAX_SCAN_BYTES {
            return Err("log query byte limit reached; narrow the time window".into());
        }
        read += count;
        if read > MAX_RECORD_BYTES + 1 {
            overflow = true;
            line.clear();
        }
        if !overflow {
            line.extend_from_slice(&buffer[..count]);
        }
        reader.consume(count);
        if newline.is_some() {
            return Ok(Some(if overflow { None } else { Some(line) }));
        }
    }
}

#[derive(Eq, PartialEq)]
struct SortedRecord(LogRecord);

impl Ord for SortedRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.0.created_at, &self.0.request_id).cmp(&(&other.0.created_at, &other.0.request_id))
    }
}

impl PartialOrd for SortedRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
