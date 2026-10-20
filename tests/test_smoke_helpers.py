"""Regression tests for delayed and fragmented PTY output."""
import unittest
from unittest.mock import Mock, patch

from smoke_helpers import wait_for_output


class OutputWaitTests(unittest.TestCase):
    def test_waits_for_delayed_startup_markers(self):
        read = Mock(side_effect=[b"", b"Inbox", b" Tasks", b" ?: keybindings"])
        output = wait_for_output(read, (b"Inbox", b"Tasks", b"?: keybindings"))
        self.assertIn(b"?: keybindings", output)
        self.assertEqual(read.call_count, 4)

    def test_accumulates_checkpoint_across_reads(self):
        read = Mock(side_effect=[b"", b"check", b"point"])
        self.assertEqual(
            wait_for_output(read, b"second checkpoint", initial=b"second "),
            b"second checkpoint",
        )

    def test_matching_initial_output_needs_no_read(self):
        read = Mock()
        wait_for_output(read, b"ready", initial=b"ready")
        read.assert_not_called()

    def test_timeout_reports_expected_and_captured_output(self):
        read = Mock(return_value=b"partial frame")
        with patch("smoke_helpers.time.monotonic", side_effect=[0, 0, 6]):
            with self.assertRaisesRegex(AssertionError, "ready.*partial frame"):
                wait_for_output(read, b"ready")
        read.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
