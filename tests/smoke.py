"""Real PTY smoke test; run after cargo build: python3 tests/smoke.py."""
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import sqlite3
import struct
import subprocess
import tempfile
import termios
import time

binary = Path(__file__).resolve().parents[1] / "target/debug/cli-todo"
with tempfile.TemporaryDirectory() as data:
    dbpath = Path(data) / "cli-todo/tasks.sqlite3"

    def rows():
        with sqlite3.connect(dbpath) as db:
            return db.execute("SELECT title,done FROM tasks ORDER BY id").fetchall()

    def launch():
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 110, 0, 0))
        before = termios.tcgetattr(slave)
        proc = subprocess.Popen([binary], stdin=slave, stdout=slave, stderr=slave,
                                env={**os.environ, "TERM": "xterm-256color", "XDG_DATA_HOME": data,
                                     "GIT_AUTHOR_NAME": "Todo Test", "GIT_AUTHOR_EMAIL": "todo@example.test",
                                     "GIT_COMMITTER_NAME": "Todo Test", "GIT_COMMITTER_EMAIL": "todo@example.test",
                                     "GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "commit.gpgsign", "GIT_CONFIG_VALUE_0": "false"})
        return proc, master, slave, before

    def screen():
        time.sleep(0.08)
        out = b""
        while select.select([master], [], [], 0.1)[0]:
            out += os.read(master, 65536)
        return b" ".join(re.sub(rb"\x1b\[[0-9;?]*[A-Za-z]", b" ", out).split())

    def send(keys):
        os.write(master, keys)
        return screen()

    def finish(expected):
        assert proc.wait(timeout=3) == expected
        assert termios.tcgetattr(slave) == before, "terminal attributes not restored"
        os.close(master)
        os.close(slave)

    proc, master, slave, before = launch()
    try:
        first = screen()
        deadline = time.monotonic() + 5
        while not all(text in first for text in [b"Inbox", b"Tasks", b"?: keybindings"]):
            assert time.monotonic() < deadline, "initial frame did not render"
            first += b" " + screen()
        assert b"Keybindings" in send(b"?")
        send(b"\x1b[F")
        send(b"\x1b[H")
        send(b"?")
        send(b"?")
        send(b"q")
        assert proc.poll() is None, "help should not execute browsing shortcuts"
        send(b"\x1b")
        assert rows() == []
        assert b"Title cannot be empty" in send(b"a\r")
        send(b"first\r")
        assert rows() == [("first", 0)]
        send(b"aabandoned\x1b")
        assert len(rows()) == 1
        send(b"\r\r!\r")
        assert rows() == [("first!", 0)]
        send(b" ")
        assert rows() == [("first!", 1)]
        send(b"\x1b")
        assert b"No tasks here" in send(b"\x1b[B")
        send(b"\r")
        send(b"\x1b")
        send(b"\x1b[B\r")
        assert b"Delete task?" in send(b"d")
        send(b"\x1b")
        assert len(rows()) == 1
        send(b"q")
        finish(0)
        proc, master, slave, before = launch()
        assert b"first!" in screen()
        send(b"\rd\r")
        assert rows() == []
        assert b"Commands" in send(b"\x0b")
        assert b"No matching tasks or actions" in send(b"zzzz")
        send(b"\x1b")
        send(b"\x0badd\rpalette task\r")
        assert rows() == [("palette task", 0)]
        send(b"\x0bpending\r")
        assert b"Task:" in send(b"\x0bpalette task")
        send(b"\r")
        send(b"\x0bedit\r!\r")
        assert rows() == [("palette task!", 0)]
        send(b"\x0breopen\r")
        assert rows() == [("palette task!", 1)]
        assert b"Delete task?" in send(b"\x0bdelete\r")
        send(b"yn")
        assert len(rows()) == 1
        send(b"\x1b")
        send(b"\x0bdelete\r\r")
        assert rows() == []
        send(b"2aWork\r")
        assert b"Work" in send(b"\r")
        send(b"aproject task\r")
        send(b"m\r")
        assert b"project task" in send(b"1\r")
        send(b"m\x1b[B\r")
        send(b"2e!\r")
        assert b"project AND its tasks" in send(b"d")
        send(b"yn")
        assert rows() == [("project task", 0)]
        send(b"\x1b")
        send(b"d\r")
        assert rows() == []
        send(b"1apreserved\r")
        assert b"No todo commits" in send(b"3")
        assert b"Commit message cannot be empty" in send(b"c\r")
        send(b"\x1b")
        committed = send(b"cfirst checkpoint\r")
        deadline = time.monotonic() + 10
        while b"snapshot committed" not in committed:
            assert time.monotonic() < deadline, f"checkpoint did not finish: {committed!r}"
            committed += b" " + screen()
        assert b"Author:" in committed
        notification = send(b"c")
        assert b"changes to commit" in notification, notification
        assert subprocess.check_output(["git", "-C", str(dbpath.parent / "history"), "rev-list", "--count", "HEAD"]).strip() == b"1"
        history = dbpath.parent / "history"
        with sqlite3.connect(history / "tasks.sqlite3") as snapshot:
            assert snapshot.execute("SELECT title,done FROM tasks").fetchall() == [("preserved", 0)]
        send(b"\r")
        send(b"\x1b")
        send(b"1\r ")
        send(b"3csecond checkpoint\r")
        deadline = time.monotonic() + 10
        while subprocess.check_output(["git", "-C", str(history), "rev-list", "--count", "HEAD"]).strip() != b"2":
            assert time.monotonic() < deadline, "second checkpoint did not finish"
            screen()
        remote = Path(data) / "remote.git"
        branch = subprocess.check_output(["git", "-C", str(history), "symbolic-ref", "--short", "HEAD"]).decode().strip()
        subprocess.run(["git", "init", "--bare", "--quiet", f"--initial-branch={branch}", str(remote)], check=True)
        send(b"r" + str(remote).encode() + b"\r")
        assert subprocess.check_output(["git", "-C", str(history), "remote", "get-url", "origin"]).decode().strip() == str(remote)
        send(b"p")
        deadline = time.monotonic() + 5
        head = subprocess.check_output(["git", "-C", str(history), "rev-parse", "HEAD"]).strip()
        while True:
            published = subprocess.run(["git", "-C", str(remote), "rev-parse", "--verify", "HEAD"], capture_output=True)
            if published.returncode == 0 and published.stdout.strip() == head:
                break
            assert time.monotonic() < deadline, "push did not publish todo checkpoints"
            time.sleep(0.05)
        screen()
        deadline = time.monotonic() + 5
        output = send(b"P")
        while b"up to date" not in output:
            assert time.monotonic() < deadline, "pull did not finish"
            output += screen()
        send(b"q")
        finish(0)
        proc, master, slave, before = launch()
        screen()
        assert b"second checkpoint" in send(b"3")
        send(b"1\r ")
        send(b"2aClear me\r\raproject to clear\r")
        assert b"remote settings?" in send(b"D")
        send(b"\x1b")
        assert len(rows()) == 2
        send(b"Dyn")
        assert len(rows()) == 2
        send(b"\x1b")
        send(b"D\r")
        assert rows() == []
        with sqlite3.connect(dbpath) as db:
            assert db.execute("SELECT count(*) FROM projects").fetchone()[0] == 0
        assert not history.exists(), "local commits and remote configuration were not removed"
        assert subprocess.check_output(["git", "-C", str(remote), "rev-parse", "HEAD"]).strip() == head, "nuke changed the remote repository"
        send(b"q")
        finish(0)
        proc, master, slave, before = launch()
        screen()
        assert rows() == []
        send(b"apreserved\r")
        with sqlite3.connect(dbpath) as db:
            db.execute("CREATE TRIGGER fail_insert BEFORE INSERT ON tasks BEGIN SELECT RAISE(ABORT, 'smoke forced error'); END")
        output = send(b"afailure\r")
        assert b"smoke forced error" in output
        finish(1)
        assert rows() == [("preserved", 0)]
        print("PASS: PTY keybindings help, navigation, palette, projects, checkpoints, sync, nuke, persistence, terminal restoration")
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
