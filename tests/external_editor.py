"""PTY integration: external-editor handoff, import/cancel, failures and recovery."""
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import shlex
import sqlite3
import struct
import subprocess
import sys
import tempfile
import termios
import time

BINARY = Path(os.environ.get("CLI_TODO_BINARY", str(Path(__file__).resolve().parents[1] / "target/debug/cli-todo"))).resolve()
FAKE = r'''
import fcntl, json, os, struct, sys, termios
from pathlib import Path
root = Path(__file__).parent
path = Path(sys.argv[-1])
assert sys.argv[1:-1] == ["--wait", "two words"]
assert all(os.isatty(fd) for fd in (0, 1, 2))
flags = termios.tcgetattr(0)[3]
assert flags & termios.ICANON and flags & termios.ECHO, "editor inherited raw mode"
assert path.stat().st_mode & 0o777 == 0o600
assert path.parent.stat().st_mode & 0o777 == 0o700
with (root / "receipts").open("a") as log:
    log.write(json.dumps({"path": str(path), "original": path.read_text()}) + "\n")
print("FAKE_EDITOR_TERMINAL", flush=True)
mode = (root / "mode").read_text()
if mode == "placeholder":
    pass
elif mode == "invalid":
    path.write_text("not a valid task file")
elif mode == "nonzero":
    path.write_text("Title: do not import\nDue date:\n\nunsaved external work\n")
    sys.exit(7)
else:
    path.write_text("Title: new tomorrow\nDue date: 2028-02-29\n\nnext week stays literal\n")
    fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 100, 0, 0))
'''


class Harness:
    def __init__(self, root, visual=None, editor="/nonexistent/ignored"):
        self.root = root
        root.mkdir()
        self.temporary = root / "private tmp"
        self.temporary.mkdir()
        fake = root / "fake editor.py"
        fake.write_text(FAKE)
        (root / "mode").write_text("success")
        if visual is None:
            visual = f'{shlex.quote(sys.executable)} {shlex.quote(str(fake))} --wait "two words"'
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 110, 0, 0))
        self.before = termios.tcgetattr(self.slave)
        self.captured = []
        self.proc = subprocess.Popen(
            [BINARY], stdin=self.slave, stdout=self.slave, stderr=self.slave,
            env={**os.environ, "TERM": "xterm-256color", "XDG_DATA_HOME": str(root),
                 "TMPDIR": str(self.temporary), "VISUAL": visual, "EDITOR": editor},
        )

    def __enter__(self):
        first = b""
        deadline = time.monotonic() + 5
        while not all(word in first for word in [b"Inbox", b"Tasks"]):
            assert time.monotonic() < deadline, "app did not start"
            first += self.screen()
        return self

    def __exit__(self, *_):
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait()
        os.close(self.master)
        os.close(self.slave)

    def screen(self):
        time.sleep(0.08)
        output = b""
        while select.select([self.master], [], [], 0.05)[0]:
            output += os.read(self.master, 65536)
        self.captured.append(output)
        return re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b" ", output)

    def send(self, keys):
        os.write(self.master, keys)
        return self.screen()

    def tasks(self):
        with sqlite3.connect(self.root / "cli-todo/tasks.sqlite3") as db:
            return db.execute("SELECT title,description,due_date FROM tasks ORDER BY id").fetchall()

    def recoveries(self):
        return list(self.temporary.glob("cli-todo-edit-*/task.txt"))

    def finish(self):
        self.send(b"q")
        assert self.proc.wait(timeout=3) == 0
        self.screen()
        assert termios.tcgetattr(self.slave) == self.before, "terminal attributes not restored"
        output = b"".join(self.captured)
        assert b"\x1b[2 q" in output and b"\x1b[0 q" in output


with tempfile.TemporaryDirectory(prefix="cli-todo external ") as directory:
    root = Path(directory)
    with Harness(root / "round trip") as app:
        app.send(b"\x07")  # Inactive while browsing.
        assert not app.recoveries()
        app.send(b"aold\r\rr")
        assert app.tasks() == [("old", "", None)]
        output = app.send(b"\x07")
        assert b"FAKE_EDITOR_TERMINAL" in b"".join(app.captured)
        assert b"new tomorrow" in output
        assert app.tasks() == [("old", "", None)], "import must not save automatically"
        assert not app.recoveries(), "successful imports must remove private temporary files"
        app.send(b"\x1b")
        assert app.tasks() == [("old", "", None)], "Esc must discard imported changes"
        app.send(b"r\x07\r")
        expected = [("new tomorrow", "next week stays literal\n", "2028-02-29")]
        assert app.tasks() == expected, "explicit date must win without stripping imported phrases"
        assert not app.recoveries()
        # External editors must not run in compact project dialogs either.
        receipts = (app.root / "receipts").read_text()
        app.send(b"2a\x07")
        app.send(b"\x1b")
        app.send(b"0")
        assert (app.root / "receipts").read_text() == receipts
        app.send(b"1\rr")
        for mode in ["invalid", "nonzero"]:
            (app.root / "mode").write_text(mode)
            output = app.send(b"\x07")
            assert b"Recovery:" in output, (mode, output, (app.root / "receipts").read_text())
            assert app.tasks() == expected
            app.send(b"\r")  # Saving unchanged original draft cannot import failed edits.
            assert app.tasks() == expected
            app.send(b"r")
        recoveries = app.recoveries()
        assert len(recoveries) == 2
        assert {path.read_text() for path in recoveries} == {
            "not a valid task file",
            "Title: do not import\nDue date:\n\nunsaved external work\n",
        }
        assert all(path.stat().st_mode & 0o777 == 0o600 for path in recoveries)
        app.send(b"\x1b")
        app.finish()
        output = b"".join(app.captured)
        assert output.count(b"\x1b[?1049l") >= 5, "external editor must leave alternate screen"
        assert output.count(b"\x1b[2 q") >= 5, "block cursor must be restored after each editor exit"

    with Harness(root / "placeholder") as app:
        (app.root / "mode").write_text("placeholder")
        app.send(b"ano deadline\x07\r")
        assert app.tasks() == [("no deadline", "", None)]
        receipt = json.loads((app.root / "receipts").read_text())
        assert "Due date: YYYY-MM-DD\n" in receipt["original"]
        assert not app.recoveries()
        app.finish()

    with Harness(root / "missing editor", visual="/nonexistent/todo-editor") as app:
        app.send(b"adraft")
        assert b"Recovery:" in app.send(b"\x07")
        assert len(app.recoveries()) == 1
        assert app.recoveries()[0].read_text().startswith("Title: draft\n")
        app.send(b"\r")
        assert app.tasks() == [("draft", "", None)]
        app.finish()

    with Harness(root / "unconfigured", visual="", editor="") as app:
        app.send(b"adraft")
        assert b"Set VISUAL or EDITOR" in app.send(b"\x07")
        assert not app.recoveries()
        assert app.tasks() == []
        app.send(b"\x1b")
        app.finish()

print("PASS: external-editor PTY handoff, quoted arguments, literal dates, draft save/cancel, recovery, permissions, resize and terminal restoration")
