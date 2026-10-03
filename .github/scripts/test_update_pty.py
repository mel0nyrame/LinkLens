#!/usr/bin/env python3
"""真实 PTY 更新验收；仅运行 cfg(test) fixture，无网络与用户历史读写。"""
import argparse
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import struct
import subprocess
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parents[2]
ESCAPES = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07]*(?:\x07|\x1b\\)")


def test_binary():
    result = subprocess.run(["cargo", "test", "--locked", "--lib", "--no-run", "--message-format=json"], cwd=ROOT, text=True, capture_output=True, check=True)
    for line in result.stdout.splitlines():
        event = json.loads(line)
        if event.get("reason") == "compiler-artifact" and event.get("profile", {}).get("test") and event.get("executable"):
            return event["executable"]
    raise RuntimeError("未找到 lib test harness")


class Session:
    def __init__(self, binary, scenario, columns, directory):
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 20, columns, 0, 0))
        self.original = termios.tcgetattr(self.slave)
        env = dict(os.environ, TERM="xterm-256color", LINKLENS_TEST_UPDATE_SCENARIO=scenario, LINKLENS_TEST_INSTALL_DIR=str(directory))
        self.proc = subprocess.Popen([binary, "--exact", "app::tests::pty_update_fixture", "--ignored", "--nocapture"], cwd=ROOT, stdin=self.slave, stdout=self.slave, stderr=self.slave, env=env, start_new_session=True)
        self.raw = bytearray()
        os.set_blocking(self.master, False)

    def text(self):
        return ESCAPES.sub(b"", self.raw).decode("utf-8", errors="replace")

    def pump(self, duration=.1):
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.master], [], [], max(0, deadline-time.monotonic()))
            if not ready:
                continue
            try:
                chunk = os.read(self.master, 65536)
                if not chunk:
                    break
                self.raw.extend(chunk)
                if b"\x1b[6n" in chunk:
                    self.send(b"\x1b[1;1R")
                if b"\x1b[c" in chunk:
                    self.send(b"\x1b[?1;2c")
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise

    def send(self, data):
        os.write(self.master, data)

    def wait(self, needle, timeout=8):
        end = time.monotonic() + timeout
        while needle not in self.text():
            self.pump()
            if time.monotonic() > end or self.proc.poll() is not None:
                raise AssertionError(f"等待 {needle!r} 失败：{self.text()[-1500:]}")

    def finish(self):
        try:
            end = time.monotonic()+8
            while self.proc.poll() is None and time.monotonic() < end:
                self.pump()
            self.pump()
            assert self.proc.poll() == 0, self.text()[-1500:]
            assert termios.tcgetattr(self.slave) == self.original, "退出后 termios 未恢复"
            assert b"\x1b[?1049l" in self.raw, "退出后未恢复主屏幕"
            assert b"\x1b[?1000l" in self.raw, "退出后未关闭鼠标捕获"
            return self.text(), bytes(self.raw)
        finally:
            if self.proc.poll() is None:
                self.proc.kill()
            self.proc.wait()
            os.close(self.master)
            os.close(self.slave)


def scenario(binary, mode, columns):
    with tempfile.TemporaryDirectory(prefix="linklens-pty-") as temp:
        directory = Path(temp)
        (directory / "linklens").write_bytes(b"old-main")
        (directory / "llens").write_bytes(b"old-short")
        session = Session(binary, mode, columns, directory)
        session.wait("LinkLens")
        session.send(b"4/203.0.113.10")
        session.wait("新版 0.2.0")
        assert not (directory/".linklens-update.lock").exists(), "自动检查不应锁安装目录"
        assert "LinkLens 更新" not in session.text(), "提示抢占了评分输入"
        session.send(b"u")
        session.pump(.2)
        assert "LinkLens 更新" not in session.text(), "评分输入 u 打开了更新弹窗"
        session.send(b"\x1b")
        session.pump(.2)
        session.send(b"1u")
        session.wait("LinkLens 更新")
        session.send(b"\x1b")
        session.pump(.2)
        assert session.proc.poll() is None, "Esc 关闭更新弹窗时退出了应用"
        session.send(b"\x1b[F")
        session.pump(.2)
        session.send(b"\x1b[H\x1b[<35;10;10M\x1b[<65;10;10M")
        session.pump(.2)
        session.send(b"u\t\r")
        session.wait("正在下载并校验")
        if mode == "cancel":
            # 下载时重新进入评分编辑，字符仍由输入框消费。
            session.send(b"4/203.0.113.10u")
            session.pump(.2)
        if mode == "latest":
            session.wait("已是最新稳定版")
            assert session.proc.poll() is None, "最新版本不应报告安装成功后退出"
            session.send(b"q")
        elif mode == "failure":
            session.wait("测试下载校验失败")
            session.send(b"\x1b")
            session.pump(.2)
            assert session.proc.poll() is None, "下载失败不能返回页面"
            session.send(b"q")
        elif mode == "cancel":
            session.send(b"\x1b")
            session.pump(3.3)
            assert session.proc.poll() is None, "取消后迟到结果退出或安装"
            with (directory/".linklens-update.lock").open("r+b") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                fcntl.flock(lock, fcntl.LOCK_UN)
            session.send(b"\x1b")
            session.pump(.2)
            session.send(b"1\x1b[F")
            session.pump(.2)
            session.send(b"q")
        elif mode == "quit":
            session.send(b"q")
        text, raw = session.finish()
        assert "input=203.0.113.10u" in text, "评分 u 未被编辑器保留"
        if mode in ("success", "stale"):
            expected = "0.3.0" if mode == "stale" else "0.2.0"
            assert f"已更新到 LinkLens {expected}" in text, "安装使用了 TUI 缓存的旧版本"
            assert raw.index(b"\x1b[?1049l") < raw.index("已更新到 LinkLens".encode()), "结果在恢复终端前输出"
            assert (directory/"linklens").read_bytes() == b"new-main"
            assert (directory/"llens").read_bytes() == b"new-short"
        else:
            assert "已更新到" not in text
            assert (directory/"linklens").read_bytes() == b"old-main"
            assert (directory/"llens").read_bytes() == b"old-short"
        assert not (directory/".linklens-update").exists(), "残留更新暂存"
        if mode == "cancel":
            match = re.search(r"PTY_RESULT .*scroll=(\d+)", text)
            assert match and int(match[1]) > 0, "End 没有移动到正文末尾"
        print(f"PASS {columns}x20 {mode}: 评分输入、弹窗、下载、滚动与退出恢复")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", help="已构建的 lib test harness 绝对路径")
    args = parser.parse_args()
    binary = args.binary or test_binary()
    for columns in (80, 120, 200):
        for mode in ("failure", "cancel", "quit", "success", "stale", "latest"):
            scenario(binary, mode, columns)


if __name__ == "__main__":
    main()
