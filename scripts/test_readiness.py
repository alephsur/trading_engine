"""Regression coverage for connection resets observed during Compose startup."""
import json
import unittest
import urllib.error
from unittest.mock import Mock

from readiness import wait_ready


class ReadinessTests(unittest.TestCase):
    def test_reset_and_unavailable_then_ready(self):
        ready = {"status": "ready", "mode": "PAPER"}
        get = Mock(side_effect=[
            ConnectionResetError(104, "Connection reset by peer"),
            urllib.error.HTTPError("http://localhost/health", 503, "starting", {}, None),
            {"status": "degraded"},
            ready,
        ])
        self.assertEqual(wait_ready(get, attempts=4, delay=0), ready)
        self.assertEqual(get.call_count, 4)

    def test_persistent_transport_failure_exhausts_budget(self):
        for error in (ConnectionResetError(), TimeoutError(), urllib.error.URLError("refused")):
            with self.subTest(error=type(error).__name__):
                get = Mock(side_effect=error)
                with self.assertRaisesRegex(SystemExit, "within 3 attempts"):
                    wait_ready(get, attempts=3, delay=0)
                self.assertEqual(get.call_count, 3)

    def test_degraded_service_never_passes(self):
        with self.assertRaisesRegex(SystemExit, "not ready"):
            wait_ready(lambda: {"status": "degraded"}, attempts=2, delay=0)

    def test_client_errors_and_invalid_responses_fail_immediately(self):
        for error in (
            urllib.error.HTTPError("http://localhost/health", 401, "unauthorized", {}, None),
            json.JSONDecodeError("invalid", "", 0),
            KeyError("status"),
        ):
            with self.subTest(error=type(error).__name__):
                get = Mock(side_effect=error)
                with self.assertRaises(type(error)):
                    wait_ready(get, attempts=3, delay=0)
                self.assertEqual(get.call_count, 1)


if __name__ == "__main__":
    unittest.main()
