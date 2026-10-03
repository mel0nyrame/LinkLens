#![cfg(unix)]
use network_tui::{
    update::{BuildIdentity, verify_installed_identity},
    update_install::UpdateInstallation,
};
use std::{fs, os::unix::fs::PermissionsExt};

#[tokio::test]
async fn running_old_image_cannot_update_a_newer_disk_installation() {
    let dir = std::env::temp_dir().join(format!("linklens-disk-identity-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let program = dir.join("linklens");
    fs::write(
        &program,
        "#!/bin/sh\nprintf '%s' '{\"version\":\"0.3.0\",\"target\":\"aarch64-apple-darwin\"}'\n",
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let loaded = BuildIdentity {
        version: "0.1.1".into(),
        target: "aarch64-apple-darwin".into(),
    };
    let (guard, _) = UpdateInstallation::begin(&program).unwrap();
    let rejected = verify_installed_identity(&loaded, &guard).await;
    drop(guard);
    fs::remove_dir_all(&dir).unwrap();
    assert!(rejected.unwrap_err().contains("重新启动"));
}

#[tokio::test]
async fn existing_commands_must_agree_and_missing_sibling_can_be_completed() {
    let dir =
        std::env::temp_dir().join(format!("linklens-sibling-identity-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let program = dir.join("linklens");
    let identity =
        "#!/bin/sh\nprintf '%s' '{\"version\":\"0.1.1\",\"target\":\"aarch64-apple-darwin\"}'\n";
    fs::write(&program, identity).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let loaded = BuildIdentity {
        version: "0.1.1".into(),
        target: "aarch64-apple-darwin".into(),
    };
    let (guard, _) = UpdateInstallation::begin(&program).unwrap();
    assert!(verify_installed_identity(&loaded, &guard).await.is_ok());
    fs::write(dir.join("llens"), identity.replace("0.1.1", "0.3.0")).unwrap();
    fs::set_permissions(dir.join("llens"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(verify_installed_identity(&loaded, &guard).await.is_err());
    drop(guard);
    fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn cancelled_identity_probe_kills_child_and_releases_update_lock() {
    let dir = std::env::temp_dir().join(format!("linklens-cancel-identity-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let program = dir.join("linklens");
    let pid_file = dir.join("child.pid");
    fs::write(
        &program,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nexec /bin/sleep 30\n",
            pid_file.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let loaded = BuildIdentity {
        version: "0.1.1".into(),
        target: "aarch64-apple-darwin".into(),
    };
    let (guard, _) = UpdateInstallation::begin(&program).unwrap();
    let task = tokio::spawn(async move { verify_installed_identity(&loaded, &guard).await });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let pid = loop {
        if task.is_finished() {
            let result = task.await;
            let _ = fs::remove_dir_all(&dir);
            panic!("身份子进程尚未就绪，探测任务已结束：{result:?}");
        }
        match fs::read_to_string(&pid_file) {
            Ok(pid) if pid.trim().parse::<u32>().is_ok() => break pid,
            Ok(_) => {} // 文件创建与写入不是同一个操作，等待完整 PID。
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                task.abort();
                let _ = task.await;
                let _ = fs::remove_dir_all(&dir);
                panic!("读取身份子进程就绪信号失败：{error}");
            }
        }
        if tokio::time::Instant::now() >= deadline {
            task.abort();
            let _ = task.await;
            let _ = fs::remove_dir_all(&dir);
            panic!("身份子进程未在 10 秒内就绪");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    task.abort();
    let _ = task.await;
    let mut alive = true;
    for _ in 0..100 {
        alive = std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        if !alive {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let reacquired = UpdateInstallation::begin(&program).is_ok();
    fs::remove_dir_all(&dir).unwrap();
    assert!(!alive, "取消后身份子进程仍在运行");
    assert!(reacquired, "取消后目录锁未释放");
}
