//! 官方 Release 自更新入口；不负责安装或界面。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BuildIdentity {
    pub version: String,
    pub target: String,
}
impl BuildIdentity {
    pub fn official() -> Result<Self, String> {
        let build = Self::from_release_marker(
            option_env!("LINKLENS_RELEASE_VERSION"),
            option_env!("LINKLENS_RELEASE_TARGET"),
            env!("CARGO_PKG_VERSION"),
        )?;
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("linux", "aarch64") if cfg!(target_env = "musl") => "aarch64-unknown-linux-musl",
            ("linux", "x86_64") if cfg!(target_env = "musl") => "x86_64-unknown-linux-musl",
            ("windows", "x86_64") if cfg!(target_env = "msvc") => "x86_64-pc-windows-msvc",
            _ => return Err("本机构建目标不支持官方自更新".into()),
        };
        if build.target != target {
            return Err("官方构建标识与实际平台不符".into());
        }
        Ok(build)
    }

    pub fn from_release_marker(
        version: Option<&str>,
        target: Option<&str>,
        package: &str,
    ) -> Result<Self, String> {
        let (Some(version), Some(target)) = (version, target) else {
            return Err("源码构建不支持自更新，请按原安装方式升级".into());
        };
        if crate::detect::update::stable_version(version)?
            != crate::detect::update::stable_version(package)?
        {
            return Err("官方发布标识与程序版本不一致".into());
        }
        crate::detect::update::archive_name(version, target)?;
        Ok(Self {
            version: package.into(),
            target: target.into(),
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableUpdate {
    tag: String,
    target: String,
}
impl AvailableUpdate {
    /// 指定已检查的稳定发布；不接受自定义服务地址。
    pub fn for_release(tag: &str, target: &str) -> Result<Self, String> {
        crate::detect::update::archive_name(tag, target)?;
        Ok(Self {
            tag: tag.into(),
            target: target.into(),
        })
    }
    pub fn version(&self) -> &str {
        self.tag.trim_start_matches('v')
    }
    pub fn target(&self) -> &str {
        &self.target
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckResult {
    UpToDate,
    Available(AvailableUpdate),
}

#[derive(Debug)]
pub struct DownloadedUpdate {
    pub version: String,
    pub linklens: Vec<u8>,
    pub llens: Vec<u8>,
}

pub fn auto_check_enabled() -> bool {
    std::env::var("LINKLENS_NO_UPDATE_CHECK").as_deref() != Ok("1")
}

pub async fn check(build: &BuildIdentity) -> Result<CheckResult, String> {
    Service::official().check(build).await
}
pub async fn download(update: &AvailableUpdate) -> Result<DownloadedUpdate, String> {
    Service::official().download(update).await
}

struct Service {
    client: reqwest::Client,
    api: String,
    downloads: String,
}
impl Service {
    fn official() -> Self {
        Self {
            client: crate::net::http::client(),
            api: "https://api.github.com/repos/mel0nyrame/LinkLens/releases/latest".into(),
            downloads: "https://github.com/mel0nyrame/LinkLens/releases/download".into(),
        }
    }
    async fn check(&self, build: &BuildIdentity) -> Result<CheckResult, String> {
        use crate::net::update::{CHECK_TIMEOUT, MAX_METADATA_BYTES, Release, fetch};
        crate::detect::update::archive_name(&format!("v{}", build.version), &build.target)?;
        let bytes = fetch(&self.client, &self.api, CHECK_TIMEOUT, MAX_METADATA_BYTES).await?;
        let release: Release =
            serde_json::from_slice(&bytes).map_err(|e| format!("更新版本响应无效：{e}"))?;
        if release.draft || release.prerelease {
            return Err("更新服务返回了非稳定发布".into());
        }
        let update = AvailableUpdate::for_release(&release.tag_name, &build.target)?;
        if !crate::detect::update::is_newer_stable(&build.version, &release.tag_name)? {
            return Ok(CheckResult::UpToDate);
        }
        let name = crate::detect::update::archive_name(&update.tag, &update.target)?;
        for name in [name.as_str(), "SHA256SUMS"] {
            let mut found = release.assets.iter().filter(|a| a.name == name);
            let asset = found
                .next()
                .ok_or_else(|| format!("Release 缺少附件：{name}"))?;
            let expected = format!("{}/{}/{name}", self.downloads, update.tag);
            if found.next().is_some() || asset.browser_download_url != expected {
                return Err(format!("Release 附件重复或来源不符：{name}"));
            }
        }
        Ok(CheckResult::Available(update))
    }
    async fn download(&self, update: &AvailableUpdate) -> Result<DownloadedUpdate, String> {
        use crate::net::update::{DOWNLOAD_TIMEOUT, MAX_METADATA_BYTES, fetch};
        let name = crate::detect::update::archive_name(&update.tag, &update.target)?;
        let base = format!("{}/{}", self.downloads, update.tag);
        let client = self.client.clone();
        let target = update.target.clone();
        let version = update.version().to_owned();
        tokio::time::timeout(DOWNLOAD_TIMEOUT, async move {
            let sums = fetch(
                &client,
                &format!("{base}/SHA256SUMS"),
                DOWNLOAD_TIMEOUT,
                MAX_METADATA_BYTES,
            )
            .await?;
            let sums = String::from_utf8(sums).map_err(|_| "校验清单不是 UTF-8")?;
            let bytes = fetch(
                &client,
                &format!("{base}/{name}"),
                DOWNLOAD_TIMEOUT,
                crate::detect::update::MAX_PROGRAM_BYTES,
            )
            .await?;
            let programs = tokio::task::spawn_blocking(move || {
                crate::detect::update::verified_programs(&bytes, &sums, &name, &target)
            })
            .await
            .map_err(|e| format!("解包任务失败：{e}"))??;
            Ok(DownloadedUpdate {
                version,
                linklens: programs.linklens,
                llens: programs.llens,
            })
        })
        .await
        .map_err(|_| "更新下载超时（180 秒）".to_owned())?
    }
}

/// 锁保护下核对磁盘程序身份，拒绝沿被替换的路径使用旧进程版本更新。
pub async fn verify_installed_identity(
    build: &BuildIdentity,
    installation: &crate::update_install::UpdateInstallation,
) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    for program in installation.programs().map_err(|e| e.to_string())? {
        let disk = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut child = tokio::process::Command::new(&program)
                .arg("--linklens-update-identity")
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("无法核对磁盘程序身份：{e}"))?;
            let mut bytes = Vec::new();
            child
                .stdout
                .take()
                .ok_or("无法读取程序身份")?
                .take(4097)
                .read_to_end(&mut bytes)
                .await
                .map_err(|e| e.to_string())?;
            if bytes.len() > 4096 {
                return Err("程序身份响应超出大小限制".to_owned());
            }
            let status = child.wait().await.map_err(|e| e.to_string())?;
            if !status.success() {
                return Err("安装目录程序不支持官方更新身份核对，请按原安装方式修复".to_owned());
            }
            serde_json::from_slice::<BuildIdentity>(&bytes)
                .map_err(|e| format!("程序身份响应无效：{e}"))
        })
        .await
        .map_err(|_| "磁盘程序身份核对超时（5 秒）".to_owned())??;
        if &disk != build {
            return Err(format!(
                "安装目录中的 {} 已变化或两个命令身份不一致；请重新启动该目录中的命令，必要时按原安装方式修复",
                program.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(replies: Vec<Vec<u8>>) -> Service {
        fixture_with(|_| replies)
    }
    fn fixture_with(make: impl FnOnce(&str) -> Vec<Vec<u8>>) -> Service {
        fixture_capture(make).0
    }
    fn fixture_capture(
        make: impl FnOnce(&str) -> Vec<Vec<u8>>,
    ) -> (Service, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let replies = make(&format!("http://{address}/download"));
        let paths = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = paths.clone();
        std::thread::spawn(move || {
            for body in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let count = stream.read(&mut request).unwrap();
                let path = std::str::from_utf8(&request[..count])
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_owned();
                recorded.lock().unwrap().push(path);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                let _ = stream.write_all(&body);
            }
        });
        (
            Service {
                client: reqwest::Client::builder().no_proxy().build().unwrap(),
                api: format!("http://{address}/latest"),
                downloads: format!("http://{address}/download"),
            },
            paths,
        )
    }
    #[tokio::test]
    async fn check_failure_is_distinct_from_latest() {
        let service = fixture(vec![b"invalid JSON".to_vec()]);
        let build = BuildIdentity::from_release_marker(
            Some("v0.1.1"),
            Some("aarch64-apple-darwin"),
            "0.1.1",
        )
        .unwrap();
        assert!(service.check(&build).await.is_err());
    }
    fn archive() -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        for (name, bytes) in [
            ("linklens", b"main".as_slice()),
            ("llens", b"short".as_slice()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }
    #[tokio::test]
    async fn pinned_release_download_returns_verified_programs() {
        use sha2::Digest;
        let bytes = archive();
        let name = "linklens-v0.2.0-aarch64-apple-darwin.tar.gz";
        let sums = format!("{:x}  {name}\n", sha2::Sha256::digest(&bytes));
        let (service, paths) = fixture_capture(|base| {
            vec![serde_json::to_vec(&serde_json::json!({
            "tag_name": "v0.2.0", "draft": false, "prerelease": false,
            "assets": [{"name": name, "browser_download_url": format!("{base}/v0.2.0/{name}")}, {"name":"SHA256SUMS", "browser_download_url":format!("{base}/v0.2.0/SHA256SUMS")}]
        })).unwrap(), sums.into_bytes(), bytes]
        });
        let build = BuildIdentity::from_release_marker(
            Some("v0.1.1"),
            Some("aarch64-apple-darwin"),
            "0.1.1",
        )
        .unwrap();
        let CheckResult::Available(update) = service.check(&build).await.unwrap() else {
            panic!("应发现新版")
        };
        assert_eq!(update.version(), "0.2.0");
        let downloaded = service.download(&update).await.unwrap();
        assert_eq!(downloaded.version, "0.2.0");
        assert_eq!(downloaded.linklens, b"main");
        assert_eq!(downloaded.llens, b"short");
        assert_eq!(
            *paths.lock().unwrap(),
            [
                "/latest",
                "/download/v0.2.0/SHA256SUMS",
                "/download/v0.2.0/linklens-v0.2.0-aarch64-apple-darwin.tar.gz"
            ]
        );
    }
    #[tokio::test]
    async fn check_rejects_bad_source_missing_asset_and_nonstable_release() {
        let build = BuildIdentity::from_release_marker(
            Some("v0.1.1"),
            Some("aarch64-apple-darwin"),
            "0.1.1",
        )
        .unwrap();
        for release in [
            serde_json::json!({"tag_name":"v0.2.0", "draft":false,"prerelease":false,"assets":[]}),
            serde_json::json!({"tag_name":"v0.2.0", "draft":false,"prerelease":true,"assets":[]}),
            serde_json::json!({"tag_name":"v0.2.0-rc.1", "draft":false,"prerelease":false,"assets":[]}),
            serde_json::json!({"tag_name":"v0.2.0", "draft":false,"prerelease":false,"assets":[{"name":"linklens-v0.2.0-aarch64-apple-darwin.tar.gz", "browser_download_url":"http://example.invalid/rogue"}]}),
        ] {
            let service = fixture(vec![serde_json::to_vec(&release).unwrap()]);
            assert!(service.check(&build).await.is_err());
        }
        for version in ["v0.1.1", "v0.1.0"] {
            let service = fixture(vec![serde_json::to_vec(&serde_json::json!({"tag_name":version,"draft":false,"prerelease":false,"assets":[]})).unwrap()]);
            assert_eq!(service.check(&build).await.unwrap(), CheckResult::UpToDate);
        }
    }
    #[tokio::test]
    async fn metadata_and_manifest_size_limits_reject_before_processing() {
        let service = fixture(vec![vec![b'x'; crate::net::update::MAX_METADATA_BYTES + 1]]);
        let build = BuildIdentity::from_release_marker(
            Some("v0.1.1"),
            Some("aarch64-apple-darwin"),
            "0.1.1",
        )
        .unwrap();
        assert!(
            service
                .check(&build)
                .await
                .unwrap_err()
                .contains("大小上限")
        );
        let service = fixture(vec![vec![b'x'; crate::net::update::MAX_METADATA_BYTES + 1]]);
        let update = AvailableUpdate::for_release("v0.2.0", "aarch64-apple-darwin").unwrap();
        assert!(
            service
                .download(&update)
                .await
                .unwrap_err()
                .contains("大小上限")
        );
    }
    #[tokio::test]
    async fn download_checksum_failure_does_not_return_programs() {
        let name = "linklens-v0.2.0-aarch64-apple-darwin.tar.gz";
        let service = fixture(vec![
            format!("{}  {name}\n", "0".repeat(64)).into_bytes(),
            archive(),
        ]);
        let update = AvailableUpdate::for_release("v0.2.0", "aarch64-apple-darwin").unwrap();
        assert!(
            service
                .download(&update)
                .await
                .unwrap_err()
                .contains("SHA-256")
        );
    }

    #[test]
    fn source_or_mismatched_build_cannot_self_update() {
        assert!(BuildIdentity::from_release_marker(None, None, "0.1.1").is_err());
        assert!(
            BuildIdentity::from_release_marker(
                Some("v0.1.2"),
                Some("aarch64-apple-darwin"),
                "0.1.1"
            )
            .is_err()
        );
        assert!(
            BuildIdentity::from_release_marker(Some("v0.1.1"), Some("unknown"), "0.1.1").is_err()
        );
        let build = BuildIdentity::from_release_marker(
            Some("v0.1.1"),
            Some("aarch64-apple-darwin"),
            "0.1.1",
        )
        .unwrap();
        assert_eq!(build.version, "0.1.1");
        assert_eq!(build.target, "aarch64-apple-darwin");
    }
}
