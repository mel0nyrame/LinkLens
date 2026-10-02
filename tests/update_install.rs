use network_tui::update_install::prepare;
use std::{fs, path::PathBuf};

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct InstallDir(PathBuf);
impl InstallDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "linklens-install-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join(main_name()), b"old-main").unwrap();
        Self(path)
    }
}
impl Drop for InstallDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn main_name() -> &'static str {
    if cfg!(windows) {
        "linklens.exe"
    } else {
        "linklens"
    }
}
fn short_name() -> &'static str {
    if cfg!(windows) { "llens.exe" } else { "llens" }
}

#[cfg(not(windows))]
#[test]
fn custom_install_directory_updates_both_commands_and_fills_missing_sibling() {
    let dir = InstallDir::new();
    prepare(&dir.0.join(main_name()), b"new-main", b"new-short")
        .unwrap()
        .commit()
        .unwrap();
    assert_eq!(fs::read(dir.0.join(main_name())).unwrap(), b"new-main");
    assert_eq!(fs::read(dir.0.join(short_name())).unwrap(), b"new-short");
}

#[cfg(not(windows))]
#[test]
fn dropping_prepared_update_preserves_originals_and_allows_retry() {
    let dir = InstallDir::new();
    let pending = prepare(&dir.0.join(main_name()), b"new-main", b"new-short").unwrap();
    drop(pending);
    assert_eq!(fs::read(dir.0.join(main_name())).unwrap(), b"old-main");
    assert!(!dir.0.join(short_name()).exists());
    prepare(&dir.0.join(main_name()), b"retry-main", b"retry-short")
        .unwrap()
        .commit()
        .unwrap();
    assert_eq!(fs::read(dir.0.join(short_name())).unwrap(), b"retry-short");
}

#[cfg(unix)]
#[test]
fn sibling_symlink_is_rejected_without_touching_its_target() {
    let dir = InstallDir::new();
    fs::write(dir.0.join("outside"), b"unrelated").unwrap();
    std::os::unix::fs::symlink(dir.0.join("outside"), dir.0.join(short_name())).unwrap();
    assert!(prepare(&dir.0.join(main_name()), b"new-main", b"new-short").is_err());
    assert_eq!(fs::read(dir.0.join("outside")).unwrap(), b"unrelated");
    assert_eq!(fs::read(dir.0.join(main_name())).unwrap(), b"old-main");
}

#[test]
fn same_install_directory_rejects_concurrent_preparation() {
    let dir = InstallDir::new();
    let first = prepare(&dir.0.join(main_name()), b"first-main", b"first-short").unwrap();
    let error = match prepare(&dir.0.join(main_name()), b"second-main", b"second-short") {
        Ok(_) => panic!("并发更新必须失败"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("更新"), "{error}");
    drop(first);
    assert_eq!(fs::read(dir.0.join(main_name())).unwrap(), b"old-main");
}

#[test]
fn renamed_unrelated_executable_is_not_an_installation_target() {
    let dir = InstallDir::new();
    fs::write(dir.0.join("unrelated"), b"unrelated").unwrap();
    assert!(prepare(&dir.0.join("unrelated"), b"new-main", b"new-short").is_err());
    assert_eq!(fs::read(dir.0.join(main_name())).unwrap(), b"old-main");
}

#[cfg(unix)]
#[test]
fn read_only_install_directory_preserves_existing_commands() {
    use std::os::unix::fs::PermissionsExt;
    let dir = InstallDir::new();
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o555)).unwrap();
    let result = prepare(&dir.0.join(main_name()), b"new-main", b"new-short");
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(dir.0.join(main_name())).unwrap(), b"old-main");
}

#[cfg(unix)]
#[test]
fn recovery_refuses_untrusted_symlink_material() {
    let dir = InstallDir::new();
    let stage = dir.0.join(".linklens-update");
    fs::create_dir(&stage).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.0.join(main_name()), stage.join("journal.json")).unwrap();
        let error = network_tui::update_install::recover_installation(&dir.0).unwrap_err();
        assert!(error.recovery_dir.is_some());
        assert_eq!(fs::read(dir.0.join(main_name())).unwrap(), b"old-main");
    }
}

#[test]
fn lock_is_released_after_a_process_exits_with_prepared_files() {
    let dir = InstallDir::new();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_lock_holder", "--ignored", "--nocapture"])
        .env("LINKLENS_INSTALL_TEST_DIRECTORY", &dir.0)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = std::io::BufReader::new(child.stdout.take().unwrap());
    use std::io::BufRead;
    let mut line = String::new();
    loop {
        assert!(ready.read_line(&mut line).unwrap() > 0);
        if line.contains("LOCK_READY") {
            break;
        }
        line.clear();
    }
    assert!(prepare(&dir.0.join(main_name()), b"new-main", b"new-short").is_err());
    child.kill().unwrap();
    child.wait().unwrap();
    let retry = prepare(&dir.0.join(main_name()), b"retry-main", b"retry-short").unwrap();
    drop(retry);
    assert_eq!(fs::read(dir.0.join(main_name())).unwrap(), b"old-main");
}

#[test]
#[ignore = "由跨进程安装测试启动的子进程入口"]
fn process_lock_holder() {
    let directory = PathBuf::from(std::env::var_os("LINKLENS_INSTALL_TEST_DIRECTORY").unwrap());
    let _prepared = prepare(&directory.join(main_name()), b"child-main", b"child-short").unwrap();
    println!("LOCK_READY");
    use std::io::Write;
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}

#[cfg(windows)]
#[test]
fn actual_application_worker_replaces_running_sibling_before_returning_success() {
    use std::io::BufRead;
    let dir = InstallDir::new();
    fs::copy(env!("CARGO_BIN_EXE_linklens"), dir.0.join(main_name())).unwrap();
    // 兄弟命令先承载测试子进程，保持程序映像实际被 Windows 加载。
    fs::copy(std::env::current_exe().unwrap(), dir.0.join(short_name())).unwrap();
    let mut running = std::process::Command::new(dir.0.join(short_name()))
        .args([
            "--exact",
            "running_executable_holder",
            "--ignored",
            "--nocapture",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = std::io::BufReader::new(running.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        assert!(ready.read_line(&mut line).unwrap() > 0);
        if line.contains("EXE_READY") {
            break;
        }
        line.clear();
    }
    let main = fs::read(env!("CARGO_BIN_EXE_linklens")).unwrap();
    let short = fs::read(env!("CARGO_BIN_EXE_llens")).unwrap();
    let installed = prepare(&dir.0.join(main_name()), &main, &short)
        .unwrap()
        .commit();
    let _ = running.kill();
    running.wait().unwrap();
    let result = installed.unwrap();
    assert_eq!(result.directory, dir.0.canonicalize().unwrap());
    assert_eq!(fs::read(dir.0.join(main_name())).unwrap(), main);
    assert_eq!(fs::read(dir.0.join(short_name())).unwrap(), short);
    let main_version = std::process::Command::new(dir.0.join(main_name()))
        .arg("--installation-smoke")
        .output()
        .unwrap();
    let short_version = std::process::Command::new(dir.0.join(short_name()))
        .arg("--installation-smoke")
        .output()
        .unwrap();
    assert!(!main_version.status.success());
    assert!(!short_version.status.success());
    assert_eq!(main_version.stdout, short_version.stdout);
    assert_eq!(main_version.stderr, short_version.stderr);
    assert!(!main_version.stderr.is_empty());
    network_tui::update_install::recover_installation(&dir.0).unwrap();
    assert!(!dir.0.join(".linklens-update").exists());
}

#[cfg(windows)]
#[test]
#[ignore = "由 Windows 运行中程序替换测试启动的子进程入口"]
fn running_executable_holder() {
    use std::io::Write;
    println!("EXE_READY");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}
