"""HTTP regressions without GPU/model dependencies: python3 -m unittest discover -s scripts -p test_tts_server.py"""
import concurrent.futures
import http.client
import importlib.util
from pathlib import Path
import threading
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("tts_server", Path(__file__).with_name("tts-server.py"))
tts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tts)


class TtsServerTests(unittest.TestCase):
    def test_health_remains_responsive_during_serialized_synthesis(self):
        entered = threading.Event()
        release = threading.Event()
        guard = threading.Lock()
        active = 0
        maximum = 0

        def synthesize(**kwargs):
            nonlocal active, maximum
            with guard:
                active += 1
                maximum = max(maximum, active)
            entered.set()
            self.assertTrue(release.wait(5))
            with guard:
                active -= 1
            return b"RIFF-test", 10

        server = tts.ThreadingHTTPServer(("127.0.0.1", 0), tts.TTSHandler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()

        def request(method, path, body=None, timeout=3):
            conn = http.client.HTTPConnection(*server.server_address, timeout=timeout)
            try:
                conn.request(method, path, body=body)
                response = conn.getresponse()
                return response.status, response.read()
            finally:
                conn.close()

        try:
            with patch.object(tts, "synthesize", side_effect=synthesize):
                with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                    first = pool.submit(request, "POST", "/synthesize", '{"text":"Selector!"}')
                    self.assertTrue(entered.wait(2))
                    second = pool.submit(request, "POST", "/synthesize", '{"text":"Rewind!"}')
                    try:
                        status, body = request("GET", "/health", timeout=0.25)
                        self.assertEqual(status, 200)
                        self.assertIn(b'"ok": true', body)
                    finally:
                        release.set()
                    self.assertEqual(first.result()[0], 200)
                    self.assertEqual(second.result()[0], 200)
                    self.assertEqual(maximum, 1, "model generation must stay serialized")
        finally:
            release.set()
            server.shutdown()
            server.server_close()
            worker.join()

    def test_disconnected_clients_do_not_raise_server_tracebacks(self):
        handler = object.__new__(tts.TTSHandler)
        for error in (BrokenPipeError, ConnectionResetError):
            with self.subTest(error=error):
                with patch.object(tts.BaseHTTPRequestHandler, "handle", side_effect=error):
                    handler.handle()

    def test_other_handler_errors_are_not_hidden(self):
        handler = object.__new__(tts.TTSHandler)
        with patch.object(tts.BaseHTTPRequestHandler, "handle", side_effect=ValueError):
            with self.assertRaises(ValueError):
                handler.handle()


if __name__ == "__main__":
    unittest.main()
