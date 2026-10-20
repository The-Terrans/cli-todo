"""Bounded output waits shared by the PTY smoke test and its unit tests."""
import time


def wait_for_output(read, expected, initial=b"", timeout=5):
    """Wait for all expected byte strings without sending additional input."""
    if isinstance(expected, bytes):
        expected = (expected,)
    output = initial
    deadline = time.monotonic() + timeout
    while not all(text in output for text in expected):
        assert time.monotonic() < deadline, (
            f"Timed out waiting for {expected!r}; captured output: {output!r}"
        )
        output += read()
    return output
