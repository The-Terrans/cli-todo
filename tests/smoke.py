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
                                env={**os.environ, "TERM": "xterm-256color", "XDG_DATA_HOME": data})
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
        assert b"Inbox" in first and b"Tasks" in first and b"Tab: panel" in first
        assert b"Title cannot be empty" in send(b"a\r")
        send(b"first\r")
        assert rows() == [("first", 0)]
        send(b"aabandoned\x1b")
        assert len(rows()) == 1
        send(b"\t\r!\r")
        assert rows() == [("first!", 0)]
        send(b" ")
        assert rows() == [("first!", 1)]
        assert b"No tasks here" in send(b"\t\x1b[B")
        send(b"\r")
        send(b"\t\x1b[B\r")
        assert b"Delete task?" in send(b"d")
        send(b"\x1b")
        assert len(rows()) == 1
        send(b"q")
        finish(0)
        proc, master, slave, before = launch()
        assert b"first!" in screen()
        send(b"\rdy")
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
        send(b"n")
        assert len(rows()) == 1
        send(b"\x0bdelete\ry")
        assert rows() == []
        send(b"apreserved\r")
        with sqlite3.connect(dbpath) as db:
            db.execute("CREATE TRIGGER fail_insert BEFORE INSERT ON tasks BEGIN SELECT RAISE(ABORT, 'smoke forced error'); END")
        output = send(b"afailure\r")
        assert b"smoke forced error" in output
        finish(1)
        assert rows() == [("preserved", 0)]
        print("PASS: PTY navigation, command palette search/actions, CRUD, persistence, normal/error terminal restoration")
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
