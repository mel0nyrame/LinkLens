#!/bin/sh
# Install a prebuilt LinkLens release. No Rust toolchain is required.
set -eu

main() {
    repo=mel0nyrame/LinkLens
    install_dir=${LINKLENS_INSTALL_DIR:-"$HOME/.local/bin"}
    version=${LINKLENS_VERSION:-}
    command -v curl >/dev/null 2>&1 || { echo '需要 curl。' >&2; exit 1; }
    os=$(uname -s)
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64) cpu=x86_64 ;;
        aarch64|arm64) cpu=aarch64 ;;
        *) echo "暂不支持此 CPU: $arch" >&2; exit 1 ;;
    esac
    ext=tar.gz
    suffix=
    case "$os" in
        Linux) target=$cpu-unknown-linux-musl ;;
        Darwin) target=$cpu-apple-darwin ;;
        MINGW*|MSYS*|CYGWIN*)
            [ "$cpu" = x86_64 ] || { echo 'Windows 预编译包仅支持 x64。' >&2; exit 1; }
            target=x86_64-pc-windows-msvc; ext=zip; suffix=.exe ;;
        *) echo "暂不支持此系统: $os" >&2; exit 1 ;;
    esac
    if [ "$ext" = zip ]; then
        command -v unzip >/dev/null 2>&1 || { echo '需要 unzip。' >&2; exit 1; }
    else
        command -v tar >/dev/null 2>&1 || { echo '需要 tar。' >&2; exit 1; }
    fi
    if [ -z "$version" ]; then
        latest=$(curl -fsSL --retry 3 -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")
        version=${latest##*/}
    fi
    printf '%s\n' "$version" | LC_ALL=C grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' || {
        echo '版本应使用 v1.2.3 格式，且仓库必须已有已发布的 Release。' >&2; exit 1;
    }
    asset=linklens-$version-$target.$ext
    base=https://github.com/$repo/releases/download/$version
    temp_dir=$(mktemp -d)
    trap 'rm -rf "$temp_dir"' EXIT HUP INT TERM
    echo "下载 LinkLens $version ($target)…"
    curl -fsSL --retry 3 "$base/$asset" -o "$temp_dir/$asset"
    curl -fsSL --retry 3 "$base/SHA256SUMS" -o "$temp_dir/SHA256SUMS"
    expected=$(awk -v name="$asset" '$2 == name {print $1}' "$temp_dir/SHA256SUMS")
    [ "${#expected}" -eq 64 ] || { echo '未找到有效的 SHA-256 校验值。' >&2; exit 1; }
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$temp_dir/$asset" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$temp_dir/$asset" | awk '{print $1}')
    else
        echo '需要 sha256sum 或 shasum 来验证下载。' >&2; exit 1
    fi
    [ "$actual" = "$expected" ] || { echo '下载校验失败，未安装。' >&2; exit 1; }
    if [ "$ext" = zip ]; then
        unzip -q "$temp_dir/$asset" "linklens$suffix" "llens$suffix" -d "$temp_dir"
    else
        tar -xzf "$temp_dir/$asset" -C "$temp_dir" "linklens$suffix" "llens$suffix"
    fi
    [ -f "$temp_dir/linklens$suffix" ] && [ -f "$temp_dir/llens$suffix" ] || {
        echo '安装包缺少命令文件。' >&2; exit 1;
    }
    mkdir -p "$install_dir"
    for name in linklens llens; do
        chmod 755 "$temp_dir/$name$suffix"
        cp "$temp_dir/$name$suffix" "$install_dir/.$name$suffix.new"
        mv -f "$install_dir/.$name$suffix.new" "$install_dir/$name$suffix"
    done
    echo "已安装到 $install_dir，使用 linklens 或 llens 启动。"
    case ":${PATH:-}:" in
        *":$install_dir:"*) ;;
        *) printf '请将安装目录加入 PATH（当前终端执行）：\nexport PATH="%s:$PATH"\n' "$install_dir" ;;
    esac
}
main "$@"
