//! 自更新的版本、构建目标与归档判定。

/// 解析严格稳定版本；可接受程序版本或带 v 前缀的发布标签。
pub fn stable_version(value: &str) -> Result<[u64; 3], String> {
    let text = value.strip_prefix('v').unwrap_or(value);
    let parts: Vec<_> = text.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("版本不是稳定版格式：{value}"));
    }
    let mut version = [0; 3];
    for (index, part) in parts.into_iter().enumerate() {
        if part.is_empty()
            || !part.bytes().all(|c| c.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return Err(format!("版本不是稳定版格式：{value}"));
        }
        version[index] = part.parse().map_err(|_| format!("版本数值过大：{value}"))?;
    }
    Ok(version)
}

pub fn is_newer_stable(current: &str, latest: &str) -> Result<bool, String> {
    Ok(stable_version(latest)? > stable_version(current)?)
}

pub fn archive_name(tag: &str, target: &str) -> Result<String, String> {
    stable_version(tag)?;
    let extension = match target {
        "x86_64-unknown-linux-musl"
        | "aarch64-unknown-linux-musl"
        | "x86_64-apple-darwin"
        | "aarch64-apple-darwin" => "tar.gz",
        "x86_64-pc-windows-msvc" => "zip",
        _ => return Err(format!("不支持的官方构建目标：{target}")),
    };
    if !tag.starts_with('v') {
        return Err("Release 标签必须以 v 开头".into());
    }
    Ok(format!("linklens-{tag}-{target}.{extension}"))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Programs {
    pub linklens: Vec<u8>,
    pub llens: Vec<u8>,
}

/// 归档与两个程序的总量上限。
pub const MAX_PROGRAM_BYTES: usize = 128 * 1024 * 1024;

/// 校验官方归档，仅在内存中读取允许的文件，不写入文件系统。
pub fn verified_programs(
    archive: &[u8],
    checksums: &str,
    name: &str,
    target: &str,
) -> Result<Programs, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    if archive.len() > MAX_PROGRAM_BYTES {
        return Err("更新归档超过 128 MiB".into());
    }
    if checksums.len() > crate::net::update::MAX_METADATA_BYTES {
        return Err("校验清单超过 1 MiB".into());
    }
    let mut matched = checksums.lines().filter_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?;
        let file = file.strip_prefix('*').unwrap_or(file);
        (file == name).then_some((hash, parts.next().is_none()))
    });
    let (expected, valid) = matched.next().ok_or("校验清单缺少目标归档")?;
    if !valid
        || matched.next().is_some()
        || expected.len() != 64
        || !expected.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("目标归档的校验清单无效或重复".into());
    }
    if format!("{:x}", Sha256::digest(archive)) != expected.to_ascii_lowercase() {
        return Err("更新归档 SHA-256 校验失败".into());
    }
    let windows = target == "x86_64-pc-windows-msvc";
    let extension = if windows { ".zip" } else { ".tar.gz" };
    let tag = name
        .strip_prefix("linklens-")
        .and_then(|s| s.strip_suffix(&format!("-{target}{extension}")))
        .ok_or("归档名与构建目标不符")?;
    if archive_name(tag, target)? != name {
        return Err("归档名与构建目标不符".into());
    }
    let names = if windows {
        ["linklens.exe", "llens.exe"]
    } else {
        ["linklens", "llens"]
    };
    let mut files = std::collections::HashMap::new();
    let mut total = 0usize;
    let mut collect = |name: &str, size: u64, reader: &mut dyn Read| -> Result<(), String> {
        if ![names[0], names[1], "LICENSE", "README.md"].contains(&name) || files.contains_key(name)
        {
            return Err(format!("归档包含不允许或重复的文件：{name}"));
        }
        let is_program = names.contains(&name);
        let remaining = if is_program {
            MAX_PROGRAM_BYTES.saturating_sub(total)
        } else {
            crate::net::update::MAX_METADATA_BYTES
        };
        if size > remaining as u64 {
            return Err("解包总量超过 128 MiB".into());
        }
        let mut bytes = Vec::new();
        reader
            .take(remaining as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("读取归档失败：{e}"))?;
        if bytes.len() > remaining || bytes.len() as u64 != size {
            return Err("归档文件大小无效".into());
        }
        if is_program {
            total += bytes.len();
        }
        files.insert(name.to_owned(), bytes);
        Ok(())
    };
    if windows {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive))
            .map_err(|e| format!("ZIP 无效：{e}"))?;
        if zip.len() > 4 {
            return Err("归档文件数量超过上限".into());
        }
        for index in 0..zip.len() {
            let mut file = zip.by_index(index).map_err(|e| e.to_string())?;
            let mode = file.unix_mode().unwrap_or(0) & 0o170000;
            if file.is_dir() || !matches!(mode, 0 | 0o100000) {
                return Err("归档不允许目录、链接或特殊文件".into());
            }
            let name = file.name().to_owned();
            collect(&name, file.size(), &mut file)?;
        }
    } else {
        // 限制解压流也约束扩展头的分配和未知条目的跳过成本。
        let inflated_limit = MAX_PROGRAM_BYTES + 2 * crate::net::update::MAX_METADATA_BYTES + 65536;
        let decoder = flate2::read::GzDecoder::new(archive).take(inflated_limit as u64 + 1);
        let mut tar = tar::Archive::new(decoder);
        for entry in tar.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            if !entry.header().entry_type().is_file() {
                return Err("归档不允许目录、链接或特殊文件".into());
            }
            let name = std::str::from_utf8(&entry.path_bytes())
                .map_err(|_| "归档文件名不是 UTF-8")?
                .to_owned();
            collect(&name, entry.size(), &mut entry)?;
        }
        let mut decoder = tar.into_inner();
        std::io::copy(&mut decoder, &mut std::io::sink())
            .map_err(|e| format!("gzip 流无效：{e}"))?;
        if decoder.limit() == 0 {
            return Err("解压流超过大小上限".into());
        }
    }
    let linklens = files
        .remove(names[0])
        .filter(|b| !b.is_empty())
        .ok_or("归档缺少 linklens 程序")?;
    let llens = files
        .remove(names[1])
        .filter(|b| !b.is_empty())
        .ok_or("归档缺少 llens 程序")?;
    Ok(Programs { linklens, llens })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }
    fn sums(bytes: &[u8], name: &str) -> String {
        use sha2::Digest;
        format!("{:x}  {name}\n", sha2::Sha256::digest(bytes))
    }
    #[test]
    fn validated_release_archive_returns_both_programs() {
        let target = "aarch64-apple-darwin";
        let name = "linklens-v0.2.0-aarch64-apple-darwin.tar.gz";
        let bytes = archive(&[
            ("linklens", b"main"),
            ("llens", b"short"),
            ("README.md", b"docs"),
        ]);
        assert_eq!(
            verified_programs(&bytes, &sums(&bytes, name), name, target).unwrap(),
            Programs {
                linklens: b"main".to_vec(),
                llens: b"short".to_vec()
            }
        );
    }

    #[test]
    fn corrupt_or_incomplete_archive_is_rejected() {
        let name = "linklens-v0.2.0-aarch64-apple-darwin.tar.gz";
        let target = "aarch64-apple-darwin";
        let good = archive(&[("linklens", b"main"), ("llens", b"short")]);
        assert!(
            verified_programs(
                &good,
                &format!("{}  {name}\n", "0".repeat(64)),
                name,
                target
            )
            .is_err()
        );
        for entries in [
            vec![("linklens", b"main".as_slice())],
            vec![
                ("linklens", b"main".as_slice()),
                ("llens", b"short".as_slice()),
                ("llens", b"duplicate".as_slice()),
            ],
            vec![
                ("linklens", b"main".as_slice()),
                ("llens", b"short".as_slice()),
                ("sub/file", b"bad".as_slice()),
            ],
        ] {
            let bytes = archive(&entries);
            assert!(verified_programs(&bytes, &sums(&bytes, name), name, target).is_err());
        }
    }

    #[test]
    fn tar_links_traversal_and_excess_size_are_rejected() {
        let name = "linklens-v0.2.0-aarch64-apple-darwin.tar.gz";
        let target = "aarch64-apple-darwin";
        for (path, kind, size) in [
            ("../linklens", tar::EntryType::Regular, 0),
            ("linklens", tar::EntryType::Symlink, 0),
            (
                "linklens",
                tar::EntryType::Regular,
                MAX_PROGRAM_BYTES as u64 + 1,
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
            header.set_entry_type(kind);
            header.set_size(size);
            header.set_mode(0o755);
            header.set_cksum();
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            use std::io::Write;
            encoder.write_all(header.as_bytes()).unwrap();
            encoder.write_all(&[0; 1024]).unwrap();
            let bytes = encoder.finish().unwrap();
            assert!(
                verified_programs(&bytes, &sums(&bytes, name), name, target).is_err(),
                "{path}"
            );
        }
    }
    #[test]
    fn windows_zip_only_returns_regular_programs() {
        use std::io::{Cursor, Write};
        let name = "linklens-v0.2.0-x86_64-pc-windows-msvc.zip";
        let target = "x86_64-pc-windows-msvc";
        for bad in [None, Some("../llens.exe"), Some("symlink")] {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer.start_file("linklens.exe", options).unwrap();
            writer.write_all(b"main").unwrap();
            if bad == Some("symlink") {
                writer
                    .add_symlink("llens.exe", "linklens.exe", options)
                    .unwrap();
            } else {
                writer
                    .start_file(bad.unwrap_or("llens.exe"), options)
                    .unwrap();
                writer.write_all(b"short").unwrap();
            }
            let bytes = writer.finish().unwrap().into_inner();
            let result = verified_programs(&bytes, &sums(&bytes, name), name, target);
            if bad.is_some() {
                assert!(result.is_err());
            } else {
                assert_eq!(
                    result.unwrap(),
                    Programs {
                        linklens: b"main".to_vec(),
                        llens: b"short".to_vec()
                    }
                );
            }
        }
    }
    #[test]
    fn target_and_checksum_manifest_are_bound_to_one_archive() {
        let name = "linklens-v0.2.0-aarch64-apple-darwin.tar.gz";
        let bytes = archive(&[("linklens", b"main"), ("llens", b"short")]);
        let correct = sums(&bytes, name);
        assert!(
            verified_programs(
                &bytes,
                &(correct.clone() + &correct),
                name,
                "aarch64-apple-darwin"
            )
            .is_err()
        );
        assert!(verified_programs(&bytes, &correct, name, "x86_64-apple-darwin").is_err());
        assert!(verified_programs(&bytes, "", name, "aarch64-apple-darwin").is_err());
        for target in [
            "x86_64-unknown-linux-musl",
            "aarch64-unknown-linux-musl",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
        ] {
            assert!(archive_name("v1.0.0", target).is_ok());
        }
    }

    #[test]
    fn upgrade_only_accepts_strictly_newer_stable_versions() {
        assert_eq!(is_newer_stable("0.1.1", "v0.2.0"), Ok(true));
        assert_eq!(is_newer_stable("0.1.1", "v0.1.1"), Ok(false));
        assert_eq!(is_newer_stable("1.0.0", "v0.9.9"), Ok(false));
        for tag in ["v1.0.0-rc.1", "v1.0.0+local", "v01.0.0", "v1.0", "v1.0.0.0"] {
            assert!(is_newer_stable("0.1.1", tag).is_err(), "{tag}");
        }
    }
}
