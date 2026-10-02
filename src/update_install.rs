//! 安装目录内两个命令的暂存与替换；调用者只提供已经校验的程序字节。
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Serialize, Deserialize, PartialEq)]
enum Phase {
    Prepared,
    Installing,
    Complete,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    phase: Phase,
    originals: [bool; 2],
}

#[derive(Debug)]
pub struct InstallError {
    pub message: String,
    pub recovery_dir: Option<PathBuf>,
}
impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(path) = &self.recovery_dir {
            write!(f, "；恢复材料保留在 {}", path.display())?;
        }
        Ok(())
    }
}
impl std::error::Error for InstallError {}
impl From<io::Error> for InstallError {
    fn from(value: io::Error) -> Self {
        Self {
            message: value.to_string(),
            recovery_dir: None,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum RecoveryOutcome {
    NoRollback,
    RolledBack,
}
#[derive(Debug)]
pub struct InstallOutcome {
    pub directory: PathBuf,
}
#[derive(Debug)]
pub struct PreparedInstall {
    directory: PathBuf,
    staging: PathBuf,
    lock: Option<fs::File>,
    originals: [bool; 2],
    preserve: bool,
    #[cfg(windows)]
    current_exe: PathBuf,
}
impl Drop for PreparedInstall {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = cleanup_stage(&self.staging);
        }
    }
}
fn names() -> [&'static str; 2] {
    if cfg!(windows) {
        ["linklens.exe", "llens.exe"]
    } else {
        ["linklens", "llens"]
    }
}
/// 从显式更新开始持有安装目录锁，直到取消或最终替换完成。
#[derive(Debug)]
pub struct UpdateInstallation {
    current_exe: PathBuf,
    directory: PathBuf,
    lock: fs::File,
}
impl UpdateInstallation {
    pub fn begin(current_exe: &Path) -> Result<(Self, RecoveryOutcome), InstallError> {
        let actual = current_exe.canonicalize()?;
        if !names()
            .iter()
            .any(|name| actual.file_name() == Some(std::ffi::OsStr::new(name)))
        {
            return Err(
                io::Error::other("当前程序名称不是 linklens 或 llens，无法确定安装目标").into(),
            );
        }
        let directory = actual
            .parent()
            .ok_or_else(|| io::Error::other("无法确定安装目录"))?
            .to_path_buf();
        let lock = acquire_lock(&directory)?;
        let recovered = recover_locked(&directory)?;
        Ok((
            Self {
                current_exe: actual,
                directory,
                lock,
            },
            recovered,
        ))
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn programs(&self) -> Result<Vec<PathBuf>, InstallError> {
        let mut programs = Vec::new();
        for name in names() {
            let path = self.directory.join(name);
            if validate_regular(&path)? {
                programs.push(path);
            }
        }
        Ok(programs)
    }
    /// 将流程锁直接转交暂存结果；没有解锁再上锁的窗口。
    pub fn prepare(self, linklens: &[u8], llens: &[u8]) -> Result<PreparedInstall, InstallError> {
        if linklens.is_empty() || llens.is_empty() {
            return Err(io::Error::other("两个新程序均不能为空").into());
        }
        let Self {
            current_exe: actual,
            directory,
            lock,
        } = self;
        #[cfg(not(windows))]
        let _ = actual;
        let originals = [
            validate_regular(&directory.join(names()[0]))?,
            validate_regular(&directory.join(names()[1]))?,
        ];
        let staging = directory.join(".linklens-update");
        fs::create_dir(&staging)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))?;
        }
        let prepared = PreparedInstall {
            directory,
            staging,
            lock: Some(lock),
            originals,
            preserve: false,
            #[cfg(windows)]
            current_exe: actual,
        };
        write_journal(
            &prepared.staging,
            &Journal {
                phase: Phase::Prepared,
                originals,
            },
        )?;
        for (name, bytes) in names().into_iter().zip([linklens, llens]) {
            let mut file = fs::File::create(prepared.staging.join(format!("{name}.new")))?;
            file.write_all(bytes)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(0o755))?;
            }
            file.sync_all()?;
        }
        sync_directory(&prepared.staging)?;
        Ok(prepared)
    }
}
/// 已校验字节的底层安装入口；生产更新使用 UpdateInstallation 覆盖检查和下载阶段。
pub fn prepare(
    current_exe: &Path,
    linklens: &[u8],
    llens: &[u8],
) -> Result<PreparedInstall, InstallError> {
    UpdateInstallation::begin(current_exe)?
        .0
        .prepare(linklens, llens)
}

impl PreparedInstall {
    pub fn commit(self) -> Result<InstallOutcome, InstallError> {
        #[cfg(not(windows))]
        {
            self.commit_using(|from, to| fs::rename(from, to))
        }
        #[cfg(windows)]
        {
            self.commit_worker()
        }
    }
    fn commit_using(
        mut self,
        mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> Result<InstallOutcome, InstallError> {
        let _guard = &self.lock;
        write_journal(
            &self.staging,
            &Journal {
                phase: Phase::Installing,
                originals: self.originals,
            },
        )?;
        self.preserve = true;
        let result = (|| -> io::Result<()> {
            for (index, name) in names().into_iter().enumerate() {
                let target = self.directory.join(name);
                if validate_regular(&target)? != self.originals[index] {
                    return Err(io::Error::other(format!(
                        "{name} 在暂存后发生变化，停止替换"
                    )));
                }
                validate_regular(&self.staging.join(format!("{name}.new")))?;
                if self.originals[index] {
                    rename(&target, &self.staging.join(format!("{name}.old")))?;
                    sync_directory(&self.directory)?;
                    sync_directory(&self.staging)?;
                }
                rename(&self.staging.join(format!("{name}.new")), &target)?;
            }
            sync_directory(&self.directory)?;
            Ok(())
        })();
        if let Err(error) = result {
            let rollback = restore(&self.directory, &self.staging, self.originals, &mut rename);
            if let Err(rollback_error) = rollback {
                return Err(InstallError {
                    message: format!("更新失败：{error}；恢复失败：{rollback_error}"),
                    recovery_dir: Some(self.staging.clone()),
                });
            }
            self.preserve = cfg!(windows);
            return Err(error.into());
        }
        write_journal(
            &self.staging,
            &Journal {
                phase: Phase::Complete,
                originals: self.originals,
            },
        )
        .map_err(|error| InstallError {
            message: format!("两个程序已替换，但无法写入安装完成记录：{error}"),
            recovery_dir: Some(self.staging.clone()),
        })?;
        self.preserve = cfg!(windows);
        Ok(InstallOutcome {
            directory: self.directory.clone(),
        })
    }
}
fn restore(
    directory: &Path,
    staging: &Path,
    originals: [bool; 2],
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut errors = Vec::new();
    for (index, name) in names().into_iter().enumerate().rev() {
        let target = directory.join(name);
        let backup = staging.join(format!("{name}.old"));
        let result = (|| -> io::Result<()> {
            validate_regular(&target)?;
            if validate_regular(&backup)? {
                if target.exists() {
                    fs::remove_file(&target)?;
                }
                rename(&backup, &target)?;
            } else if originals[index] && !target.exists() {
                return Err(io::Error::other("原程序与其备份均缺失，需要人工恢复"));
            } else if !originals[index]
                && !staging.join(format!("{name}.new")).exists()
                && target.exists()
            {
                fs::remove_file(&target)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(format!(
                "{name}: {error}（目标={}，备份={}）",
                target.exists(),
                backup.exists()
            ));
        }
    }
    if errors.is_empty() {
        sync_directory(directory)?;
        sync_directory(staging)
    } else {
        Err(io::Error::other(errors.join("；")))
    }
}

fn validate_regular(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(io::Error::other(format!(
            "拒绝符号链接或非普通文件：{}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn open_lock(path: &Path) -> Result<fs::File, InstallError> {
    validate_regular(path)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock().map_err(|error| InstallError {
        message: format!("另一个更新正在运行，或无法锁定安装目录：{error}"),
        recovery_dir: None,
    })?;
    Ok(file)
}
fn acquire_lock(directory: &Path) -> Result<fs::File, InstallError> {
    let gate = open_lock(&directory.join(".linklens-update-handoff.lock"))?;
    let lock = open_lock(&directory.join(".linklens-update.lock"))?;
    drop(gate);
    Ok(lock)
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
fn write_journal(staging: &Path, journal: &Journal) -> io::Result<()> {
    let temporary = staging.join("journal.tmp");
    validate_regular(&temporary)?;
    validate_regular(&staging.join("journal.json"))?;
    let mut file = fs::File::create(&temporary)?;
    serde_json::to_writer(&mut file, journal)?;
    file.sync_all()?;
    fs::rename(&temporary, staging.join("journal.json"))?;
    sync_directory(staging)
}
/// 在安装目录的锁保护下恢复上次中断的替换；恢复失败时保留备份。
pub fn recover_installation(directory: &Path) -> Result<RecoveryOutcome, InstallError> {
    let directory = directory.canonicalize()?;
    let _lock = acquire_lock(&directory)?;
    recover_locked(&directory)
}
fn recover_locked(directory: &Path) -> Result<RecoveryOutcome, InstallError> {
    let staging = directory.join(".linklens-update");
    match fs::symlink_metadata(&staging) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(RecoveryOutcome::NoRollback);
        }
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
        Ok(_) => return Err(io::Error::other("恢复目录不是普通目录").into()),
        Err(error) => return Err(error.into()),
    }
    let recovery = (|| -> io::Result<RecoveryOutcome> {
        let mut outcome = RecoveryOutcome::NoRollback;
        for entry in fs::read_dir(&staging)? {
            validate_regular(&entry?.path())?;
        }
        let journal_path = staging.join("journal.json");
        if !validate_regular(&journal_path)?
            && names()
                .iter()
                .any(|name| staging.join(format!("{name}.old")).exists())
        {
            return Err(io::Error::other("恢复记录缺失，保留已有备份供人工恢复"));
        }
        if validate_regular(&journal_path)? {
            let journal: Journal = serde_json::from_slice(&fs::read(journal_path)?)?;
            if journal.phase == Phase::Installing {
                outcome = RecoveryOutcome::RolledBack;
                restore(directory, &staging, journal.originals, &mut |from, to| {
                    fs::rename(from, to)
                })?;
                sync_directory(directory)?;
            }
        }
        cleanup_stage(&staging)?;
        sync_directory(directory)?;
        Ok(outcome)
    })();
    recovery.map_err(|error| InstallError {
        message: format!("无法恢复上次更新：{error}"),
        recovery_dir: Some(staging),
    })
}

fn cleanup_stage(staging: &Path) -> io::Result<()> {
    for entry in fs::read_dir(staging)? {
        let path = entry?.path();
        if path.file_name() != Some(std::ffi::OsStr::new("journal.json")) {
            validate_regular(&path)?;
            fs::remove_file(path)?;
        }
    }
    let journal = staging.join("journal.json");
    if validate_regular(&journal)? {
        fs::remove_file(journal)?;
    }
    fs::remove_dir(staging)
}

/// 应用入口在解析普通命令前调用；辅助进程完成后以其实际结果退出。
pub fn run_helper_if_requested() -> Option<i32> {
    #[cfg(windows)]
    {
        let mut arguments = std::env::args_os().skip(1);
        if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--linklens-update-worker")) {
            return None;
        }
        let Some(directory) = arguments.next() else {
            eprintln!("更新辅助进程缺少安装目录");
            return Some(1);
        };
        if arguments.next().is_some() {
            eprintln!("更新辅助进程参数无效");
            return Some(1);
        }
        let result = worker_install(&PathBuf::from(directory));
        let report = match &result {
            Ok(_) => WorkerResult {
                error: None,
                retain_recovery: false,
            },
            Err(error) => WorkerResult {
                error: Some(error.message.clone()),
                retain_recovery: error.recovery_dir.is_some(),
            },
        };
        let mut output = std::io::stdout().lock();
        if serde_json::to_writer(&mut output, &report).is_err() || output.flush().is_err() {
            return Some(1);
        }
        Some(if result.is_ok() { 0 } else { 1 })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
#[derive(Serialize, Deserialize)]
struct WorkerResult {
    error: Option<String>,
    retain_recovery: bool,
}

#[cfg(windows)]
impl PreparedInstall {
    fn commit_worker(mut self) -> Result<InstallOutcome, InstallError> {
        use std::{
            process::Command,
            thread,
            time::{Duration, Instant},
        };
        let worker = self.staging.join("worker.exe");
        fs::copy(&self.current_exe, &worker)?;
        fs::OpenOptions::new()
            .write(true)
            .open(&worker)?
            .sync_all()?;
        let mut child = Command::new(&worker)
            .arg("--linklens-update-worker")
            .arg(&self.directory)
            .stdout(std::process::Stdio::piped())
            .spawn()?;
        let mut output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("更新辅助进程没有结果管道"))?;
        let reader = thread::spawn(move || {
            use std::io::Read;
            let mut bytes = Vec::new();
            output.read_to_end(&mut bytes).map(|_| bytes)
        });
        self.preserve = true;
        let started = Instant::now();
        while !self.staging.join("worker.ready").exists() {
            if let Some(status) = child.try_wait()? {
                return Err(InstallError {
                    message: format!("更新辅助进程未能启动安装：{status}"),
                    recovery_dir: Some(self.staging.clone()),
                });
            }
            if started.elapsed() > Duration::from_secs(30) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(InstallError {
                    message: "更新辅助进程启动超时".into(),
                    recovery_dir: Some(self.staging.clone()),
                });
            }
            thread::sleep(Duration::from_millis(20));
        }
        // worker 已持有交接锁；任何新 prepare 都无法进入主锁交接间隙。
        drop(self.lock.take());
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() > Duration::from_secs(120) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(InstallError {
                    message: "更新辅助进程安装超时，下次更新将先恢复".into(),
                    recovery_dir: Some(self.staging.clone()),
                });
            }
            thread::sleep(Duration::from_millis(20));
        };
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("更新辅助进程结果读取失败"))??;
        let result: WorkerResult =
            serde_json::from_slice(&bytes).map_err(|error| InstallError {
                message: format!("更新辅助进程结果无效：{error}"),
                recovery_dir: Some(self.staging.clone()),
            })?;
        if !status.success() || result.error.is_some() {
            return Err(InstallError {
                message: result
                    .error
                    .unwrap_or_else(|| format!("更新辅助进程失败：{status}")),
                recovery_dir: result.retain_recovery.then(|| self.staging.clone()),
            });
        }
        // 旧程序仍在执行，保留完整 Complete 记录，下一次更新清理。
        Ok(InstallOutcome {
            directory: self.directory.clone(),
        })
    }
}

#[cfg(windows)]
fn worker_install(directory: &Path) -> Result<(), InstallError> {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let directory = directory.canonicalize()?;
    let staging = directory.join(".linklens-update");
    let current_exe = std::env::current_exe()?.canonicalize()?;
    if current_exe != staging.join("worker.exe") {
        return Err(io::Error::other("拒绝非暂存目录内的更新辅助进程").into());
    }
    for entry in fs::read_dir(&staging)? {
        validate_regular(&entry?.path())?;
    }
    let journal: Journal = serde_json::from_slice(&fs::read(staging.join("journal.json"))?)
        .map_err(io::Error::other)?;
    if journal.phase != Phase::Prepared {
        return Err(io::Error::other("更新辅助进程缺少待安装记录").into());
    }
    let gate = open_lock(&directory.join(".linklens-update-handoff.lock"))?;
    let mut ready = fs::File::create(staging.join("worker.ready"))?;
    ready.write_all(b"ready")?;
    ready.sync_all()?;
    drop(ready);
    let started = Instant::now();
    let lock = loop {
        match open_lock(&directory.join(".linklens-update.lock")) {
            Ok(lock) => break lock,
            Err(error) if started.elapsed() > Duration::from_secs(30) => return Err(error),
            Err(_) => thread::sleep(Duration::from_millis(20)),
        }
    };
    drop(gate);
    let prepared = PreparedInstall {
        directory,
        staging: staging.clone(),
        lock: Some(lock),
        originals: journal.originals,
        preserve: true,
        current_exe,
    };
    prepared
        .commit_using(|from, to| fs::rename(from, to))
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_command_failure_restores_both_originals() {
        let directory =
            std::env::temp_dir().join(format!("linklens-rollback-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join(names()[0]), b"old-main").unwrap();
        fs::write(directory.join(names()[1]), b"old-short").unwrap();
        let pending = prepare(&directory.join(names()[0]), b"new-main", b"new-short").unwrap();
        let result = pending.commit_using(|from, to| {
            if from.ends_with(format!("{}.new", names()[1])) {
                return Err(io::Error::other("第二个替换失败"));
            }
            fs::rename(from, to)
        });
        assert!(result.is_err());
        assert_eq!(fs::read(directory.join(names()[0])).unwrap(), b"old-main");
        assert_eq!(fs::read(directory.join(names()[1])).unwrap(), b"old-short");
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn failed_rollback_retains_material_and_next_update_recovers() {
        let directory =
            std::env::temp_dir().join(format!("linklens-recovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join(names()[0]), b"old-main").unwrap();
        fs::write(directory.join(names()[1]), b"old-short").unwrap();
        let pending = prepare(&directory.join(names()[0]), b"new-main", b"new-short").unwrap();
        let error = pending
            .commit_using(|from, to| {
                if from.ends_with(format!("{}.new", names()[1]))
                    || from.ends_with(format!("{}.old", names()[0]))
                {
                    return Err(io::Error::other("磁盘暂时故障"));
                }
                fs::rename(from, to)
            })
            .unwrap_err();
        assert!(error.recovery_dir.as_ref().unwrap().exists());
        // 主命令在恢复失败后可能缺失；公开恢复入口接受安装目录。
        recover_installation(&directory).unwrap();
        assert_eq!(fs::read(directory.join(names()[0])).unwrap(), b"old-main");
        assert_eq!(fs::read(directory.join(names()[1])).unwrap(), b"old-short");
        let pending = prepare(&directory.join(names()[0]), b"retry-main", b"retry-short").unwrap();
        drop(pending);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn interrupted_second_replacement_recovers_before_next_preparation() {
        let directory =
            std::env::temp_dir().join(format!("linklens-interrupted-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join(names()[0]), b"old-main").unwrap();
        fs::write(directory.join(names()[1]), b"old-short").unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "update_install::tests::interruption_worker",
                "--ignored",
            ])
            .env("LINKLENS_INTERRUPTION_TEST_DIRECTORY", &directory)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86));
        assert_eq!(fs::read(directory.join(names()[0])).unwrap(), b"new-main");
        let prepared = prepare(&directory.join(names()[0]), b"retry-main", b"retry-short").unwrap();
        assert_eq!(fs::read(directory.join(names()[0])).unwrap(), b"old-main");
        assert_eq!(fs::read(directory.join(names()[1])).unwrap(), b"old-short");
        drop(prepared);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    #[ignore = "由中断恢复测试启动的子进程入口"]
    fn interruption_worker() {
        let directory =
            PathBuf::from(std::env::var_os("LINKLENS_INTERRUPTION_TEST_DIRECTORY").unwrap());
        let pending = prepare(&directory.join(names()[0]), b"new-main", b"new-short").unwrap();
        let _ = pending.commit_using(|from, to| {
            if from.ends_with(format!("{}.new", names()[1])) {
                std::process::exit(86);
            }
            fs::rename(from, to)
        });
        panic!("测试未能进入第二个程序的替换阶段");
    }
    #[test]
    fn failed_update_of_only_short_command_removes_added_primary() {
        let directory =
            std::env::temp_dir().join(format!("linklens-short-only-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join(names()[1]), b"old-short").unwrap();
        let pending = prepare(&directory.join(names()[1]), b"new-main", b"new-short").unwrap();
        assert!(
            pending
                .commit_using(|from, to| {
                    if from.ends_with(format!("{}.new", names()[1])) {
                        return Err(io::Error::other("替换失败"));
                    }
                    fs::rename(from, to)
                })
                .is_err()
        );
        assert!(!directory.join(names()[0]).exists());
        assert_eq!(fs::read(directory.join(names()[1])).unwrap(), b"old-short");
        fs::remove_dir_all(directory).unwrap();
    }
}
