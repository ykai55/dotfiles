import io
import sys
import unittest
from unittest import mock


class CapturingTestCase(unittest.TestCase):
    def setUp(self):
        super().setUp()
        self._stdout_buffer = io.StringIO()
        self._stderr_buffer = io.StringIO()
        self._stdout_patcher = mock.patch("sys.stdout", new=self._stdout_buffer)
        self._stderr_patcher = mock.patch("sys.stderr", new=self._stderr_buffer)
        self._stdout_patcher.start()
        self._stderr_patcher.start()

    def tearDown(self):
        failed = False
        outcome = getattr(self, "_outcome", None)
        result = getattr(outcome, "result", None) if outcome else None
        if result:
            for test, _ in result.failures + result.errors:
                if test is self:
                    failed = True
                    break
        if failed:
            out = self._stdout_buffer.getvalue()
            err = self._stderr_buffer.getvalue()
            stdout = sys.__stdout__ or sys.stdout
            stderr = sys.__stderr__ or sys.stderr
            if out:
                stdout.write(out)
            if err:
                stderr.write(err)
        self._stdout_patcher.stop()
        self._stderr_patcher.stop()
        super().tearDown()
