# Derived in part from duckdb/extension-template-rs (commit abb7b2f3f954a47f90fc37a99cda3ce0618e6094)

import http.server
import json
import os
import socket
import socketserver
import sys
import threading
import time
import unittest
from datetime import date
from pathlib import Path
import duckdb

FIXTURES_DIR = Path(__file__).resolve().parent.parent / "fixtures"
G1_FIXTURE_PATH = FIXTURES_DIR / "g1-response.json"

DEFAULT_EXT_PATH = (
    Path(__file__).resolve().parent.parent.parent
    / "build"
    / "test_support"
    / "duckaki.duckdb_extension"
)


class MockServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def __init__(self, server_address, RequestHandlerClass):
        super().__init__(server_address, RequestHandlerClass)
        self.request_count = 0
        self.last_path = None
        self.response_mode = "g1_normal"
        self.custom_body = None
        self.request_started = threading.Event()
        self.body_started = threading.Event()
        self.response_completed_at = None
        self.lock = threading.Lock()


class MockHandler(http.server.BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        # Silence HTTP server logs during tests
        pass

    def do_GET(self):
        with self.server.lock:
            self.server.request_count += 1
            self.server.last_path = self.path
            mode = self.server.response_mode
            custom_body = self.server.custom_body
        self.server.request_started.set()

        if mode in ("slow_body", "slow_drip"):
            if custom_body is None:
                with open(G1_FIXTURE_PATH, "rb") as f:
                    payload = f.read()
            elif isinstance(custom_body, str):
                payload = custom_body.encode("utf-8")
            else:
                payload = custom_body
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            try:
                self.server.body_started.set()
                if mode == "slow_body":
                    split = max(1, len(payload) // 2)
                    self.wfile.write(payload[:split])
                    self.wfile.flush()
                    time.sleep(0.2)
                    self.wfile.write(payload[split:])
                else:
                    for byte in payload:
                        self.wfile.write(bytes((byte,)))
                        self.wfile.flush()
                        time.sleep(0.03)
                self.server.response_completed_at = time.time()
            except OSError:
                return
            return

        if mode == "large_headers":
            self.send_response(200)
            self.send_header("X-Oversized", "x" * (32 * 1024))
            self.end_headers()
            return

        if mode == "hang":
            time.sleep(1.0)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            with open(G1_FIXTURE_PATH, "rb") as f:
                self.wfile.write(f.read())
            return

        if mode == "500":
            self.send_response(500)
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"Internal Server Error")
            return
        if mode in ("403", "429"):
            self.send_response(int(mode))
            self.send_header("Content-Type", "text/plain")
            self.end_headers()
            self.wfile.write(b"Error status")
            return

        if mode == "302":
            self.send_response(302)
            self.send_header("Location", "/redirected")
            self.end_headers()
            return

        if mode == "truncated":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"rc": 0, "data": {"code": "000001", "market": 0, "klines": ["2024-01-05,9.18,9.20')
            return

        if mode == "invalid_json":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b"NOT_JSON_DATA{{{")
            return

        if mode == "empty_klines":
            payload = json.dumps({
                "rc": 0,
                "rt": 17,
                "svr": 2887138014,
                "lt": 2,
                "full": 0,
                "dlmkts": "",
                "data": {
                    "total": 0,
                    "code": "000001",
                    "market": 0,
                    "name": "平安银行",
                    "decimal": 2,
                    "dktotal": 8500,
                    "preKPrice": 9.18,
                    "klines": [],
                },
            }).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return

        if mode == "large_body":
            # Generate body > 1 MiB (limit is 1048576)
            oversized = b"x" * (1024 * 1024 + 100)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(oversized)))
            self.end_headers()
            self.wfile.write(oversized)
            return

        if mode == "custom" and custom_body is not None:
            if isinstance(custom_body, str):
                body_bytes = custom_body.encode("utf-8")
            else:
                body_bytes = custom_body
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body_bytes)))
            self.end_headers()
            self.wfile.write(body_bytes)
            return

        # Default: g1_normal
        with open(G1_FIXTURE_PATH, "rb") as f:
            data = f.read()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


class TestDuckAkiBase(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        ext_env = os.environ.get("DUCKAKI_EXTENSION_PATH")
        if ext_env and os.path.exists(ext_env):
            cls.ext_path = os.path.abspath(ext_env)
        elif DEFAULT_EXT_PATH.exists():
            cls.ext_path = str(DEFAULT_EXT_PATH.resolve())
        else:
            # Fallback to debug path
            fallback = (
                Path(__file__).resolve().parent.parent.parent
                / "build"
                / "debug"
                / "duckaki.duckdb_extension"
            )
            if fallback.exists():
                cls.ext_path = str(fallback.resolve())
            else:
                raise RuntimeError(
                    f"Extension binary not found at {DEFAULT_EXT_PATH} or {fallback}"
                )

        cls.server = MockServer(("127.0.0.1", 0), MockHandler)
        cls.server_port = cls.server.server_address[1]
        cls.server_thread = threading.Thread(target=cls.server.serve_forever)
        cls.server_thread.daemon = True
        cls.server_thread.start()

        # Set test endpoint environment variable for test-support feature
        cls.endpoint = f"http://127.0.0.1:{cls.server_port}/kline"
        os.environ["DUCKAKI_TEST_ENDPOINT"] = cls.endpoint

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.server_thread.join(timeout=2.0)

    def setUp(self):
        with self.server.lock:
            self.server.request_count = 0
            self.server.last_path = None
            self.server.response_mode = "g1_normal"
            self.server.custom_body = None
            self.server.response_completed_at = None
            self.server.request_started.clear()
            self.server.body_started.clear()
        os.environ["DUCKAKI_TEST_ENDPOINT"] = self.endpoint
        os.environ["DUCKAKI_TEST_TIMEOUT_MS"] = "30000"

    def get_connection(self):
        con = duckdb.connect(config={"allow_unsigned_extensions": "true"})
        con.execute(f"LOAD '{self.ext_path}';")
        return con


class TestG1EngineSafety(TestDuckAkiBase):
    """G1 Engine Safety & Host Boundary verification tests."""

    def test_extension_load_and_datasets(self):
        """Extension loads and aki_datasets() works without network requests."""
        con = self.get_connection()
        res = con.execute("SELECT * FROM aki_datasets();").fetchall()
        self.assertEqual(len(res), 1)
        self.assertEqual(res[0][0], "stock_daily")
        self.assertEqual(res[0][1], "aki_stock_daily")
        self.assertEqual(res[0][2], "eastmoney")
        self.assertIn("sz000001", res[0][3])
        self.assertEqual(self.server.request_count, 0)

    def test_zero_network_during_bind_phase(self):
        """Bind phase (DESCRIBE and PREPARE) executes with strictly zero network requests."""
        con = self.get_connection()
        describe_res = con.execute(
            "DESCRIBE SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        ).fetchall()
        self.assertEqual(len(describe_res), 12)
        self.assertEqual(self.server.request_count, 0)

        con.execute(
            "PREPARE p_daily AS SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        )
        self.assertEqual(self.server.request_count, 0)

    def test_external_access_disabled_blocks_network_call(self):
        """Setting enable_external_access=false blocks aki_stock_daily with DUCKAKI_EXTERNAL_ACCESS_DISABLED."""
        con = self.get_connection()
        con.execute("SET enable_external_access=false;")

        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        err_msg = str(ctx.exception)
        self.assertIn("DUCKAKI_EXTERNAL_ACCESS_DISABLED", err_msg)
        self.assertEqual(self.server.request_count, 0)

        # aki_datasets remains accessible even when external access is disabled
        datasets = con.execute("SELECT * FROM aki_datasets();").fetchall()
        self.assertIsInstance(datasets, list)
        self.assertEqual(self.server.request_count, 0)

    def test_prepare_then_disable_external_access_blocks_execution(self):
        """Prepared statement bound before disabling external access must be blocked at execution time."""
        con = self.get_connection()
        # Bind succeeds while external access is true
        con.execute(
            "PREPARE stmt_check AS SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        )
        self.assertEqual(self.server.request_count, 0)

        # Now disable external access prior to execute
        con.execute("SET enable_external_access=false;")

        with self.assertRaises(Exception) as ctx:
            con.execute("EXECUTE stmt_check;").fetchall()
        err_msg = str(ctx.exception)
        self.assertIn("DUCKAKI_EXTERNAL_ACCESS_DISABLED", err_msg)
        self.assertEqual(self.server.request_count, 0)

    def test_host_resilience_after_error(self):
        """Host connection remains fully usable after query failure."""
        con = self.get_connection()
        # Cause an invalid argument error
        with self.assertRaises(Exception):
            con.execute(
                "SELECT * FROM aki_stock_daily('UNKNOWN', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()

        # Subsequent query on the same connection succeeds cleanly
        res = con.execute("SELECT 100 + 23 AS answer;").fetchall()
        self.assertEqual(res, [(123,)])

    def test_in_flight_interrupt_covers_headers_body_and_output(self):
        """Cancellation fails while waiting for headers, reading a body, and consuming output."""
        scenarios = [
            ("hang", False, "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"),
            ("slow_body", True, "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"),
            (
                "g1_normal",
                False,
                "SELECT sum(i) FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney') d CROSS JOIN range(1000000000) r(i);",
            ),
        ]

        for mode, wait_for_body, query in scenarios:
            with self.subTest(mode=mode):
                self.server.response_mode = mode
                self.server.request_started.clear()
                self.server.body_started.clear()
                requests_before = self.server.request_count
                con = self.get_connection()
                error_holder = []

                def worker():
                    try:
                        con.execute(query).fetchall()
                    except Exception as error:
                        error_holder.append(error)

                thread = threading.Thread(target=worker)
                thread.start()
                self.assertTrue(self.server.request_started.wait(2.0))
                if wait_for_body:
                    self.assertTrue(self.server.body_started.wait(2.0))
                elif mode == "g1_normal":
                    time.sleep(0.1)

                con.interrupt()
                thread.join(timeout=4.0)
                self.assertFalse(thread.is_alive(), "Worker thread should terminate after interrupt")
                self.assertTrue(error_holder, "Query should fail after interrupt")
                self.assertTrue(
                    any("Interrupt" in str(error) or "INTERRUPT" in str(error) for error in error_holder),
                    error_holder,
                )
                self.assertEqual(self.server.request_count - requests_before, 1)

        con_new = self.get_connection()
        res = con_new.execute("SELECT 'still alive' AS status;").fetchall()
        self.assertEqual(res, [("still alive",)])


class TestUserStory1StockDaily(TestDuckAkiBase):
    """User Story 1: Daily K-line query & 12-column verification with synthetic fixture."""

    def test_synthetic_query_12_columns(self):
        """Execute aki_stock_daily against loopback server returning G1 fixture and verify 12 columns."""
        con = self.get_connection()
        query = (
            "SELECT symbol, trade_date, open, high, low, close, volume_shares, amount, "
            "currency, source, adjustment, fetched_at "
            "FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        )
        rows = con.execute(query).fetchall()

        self.assertEqual(len(rows), 1)
        row = rows[0]
        self.assertEqual(row[0], "sz000001")
        self.assertEqual(row[1], date(2024, 1, 5))
        self.assertAlmostEqual(row[2], 9.18)   # open
        self.assertAlmostEqual(row[3], 9.25)   # high
        self.assertAlmostEqual(row[4], 9.15)   # low
        self.assertAlmostEqual(row[5], 9.20)   # close
        self.assertEqual(row[6], 1200)         # 12 hand * 100 = 1200 shares
        self.assertAlmostEqual(row[7], 12000.0)
        self.assertEqual(row[8], "CNY")
        self.assertEqual(row[9], "eastmoney")
        self.assertEqual(row[10], "none")
        self.assertIsNotNone(row[11])

        # Server received 1 request
        self.assertEqual(self.server.request_count, 1)
        self.assertIn("secid=0.000001", self.server.last_path)
        self.assertIn("beg=20240101", self.server.last_path)
        self.assertIn("end=20240105", self.server.last_path)

    def test_unconfirmed_volume_missing_marker_raises_source_data(self):
        """Unconfirmed volume/amount missing markers ('-') raise DUCKAKI_SOURCE_DATA rather than guessing."""
        fixture_unconfirmed = {
            "rc": 0,
            "rt": 17,
            "svr": 2887138014,
            "lt": 2,
            "full": 0,
            "dlmkts": "",
            "data": {
                "total": 1,
                "code": "000001",
                "market": 0,
                "name": "平安银行",
                "decimal": 2,
                "dktotal": 8500,
                "preKPrice": 9.18,
                "klines": [
                    "2024-01-05,9.18,9.20,9.25,9.15,-,-,1.09,0.22,0.02,0.01"
                ],
            },
        }
        self.server.response_mode = "custom"
        self.server.custom_body = json.dumps(fixture_unconfirmed)

        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT volume_shares, amount FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_SOURCE_DATA", str(ctx.exception))
    def test_negative_fractional_volume_fails_as_source_data(self):
        """A negative fractional source volume cannot become a positive share count."""
        self.server.response_mode = "custom"
        self.server.custom_body = json.dumps(
            {
                "rc": 0,
                "data": {
                    "code": "000001",
                    "market": 0,
                    "klines": [
                        "2024-01-05,9.18,9.20,9.25,9.15,-0.5,12000.0,1.09,0.22,0.02,0.01"
                    ],
                },
            }
        )
        con = self.get_connection()

        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-10', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_SOURCE_DATA", str(ctx.exception))
        self.assertEqual(self.server.request_count, 1)

    def test_out_of_range_bad_rows_and_duplicates_fail(self):
        """Range filtering cannot hide malformed data or duplicate dates."""
        cases = [
            ["2023-12-29,BROKEN,9.20,9.25,9.15,12,12000.0,1.09,0.22,0.02,0.01"],
            ["2023-12-29,9.18,9.20,9.25,9.15,-0.5,12000.0,1.09,0.22,0.02,0.01"],
            ["2023-12-29,9.18,9.20,9.25,9.15,12,BROKEN,1.09,0.22,0.02,0.01"],
            [
                "2023-12-29,9.18,9.20,9.25,9.15,12,12000.0,1.09,0.22,0.02,0.01",
                "2023-12-29,9.18,9.20,9.25,9.15,12,12000.0,1.09,0.22,0.02,0.01",
            ],
        ]
        con = self.get_connection()
        for klines in cases:
            self.server.response_mode = "custom"
            self.server.custom_body = json.dumps(
                {"rc": 0, "data": {"code": "000001", "market": 0, "klines": klines}}
            )
            with self.subTest(klines=klines):
                with self.assertRaises(Exception) as ctx:
                    con.execute(
                        "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-10', source='eastmoney');"
                    ).fetchall()
                self.assertIn("DUCKAKI_SOURCE_DATA", str(ctx.exception))
        self.assertEqual(self.server.request_count, 4)


    def test_empty_kline_returns_zero_rows(self):
        """Empty kline response yields 0 rows cleanly without error."""
        self.server.response_mode = "empty_klines"
        con = self.get_connection()
        rows = con.execute(
            "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        ).fetchall()
        self.assertEqual(len(rows), 0)
    def test_stock_daily_4_days_fixture(self):
        """Verify 4-day fixture rows, OHLC, volume conversion, amount, and unified fetched_at."""
        fixture_path = FIXTURES_DIR / "stock_daily.json"
        with open(fixture_path, "r", encoding="utf-8") as f:
            self.server.custom_body = f.read()
        self.server.response_mode = "custom"

        con = self.get_connection()
        rows = con.execute(
            "SELECT symbol, trade_date, open, high, low, close, volume_shares, amount, currency, source, adjustment, fetched_at "
            "FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-07', source='eastmoney') "
            "ORDER BY trade_date ASC;"
        ).fetchall()

        self.assertEqual(len(rows), 4)

        expected_path = FIXTURES_DIR / "expected_daily.json"
        with open(expected_path, "r", encoding="utf-8") as f:
            expected_rows = json.load(f)
        schema = con.execute(
            "DESCRIBE SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-07', source='eastmoney');"
        ).fetchall()
        self.assertEqual(
            [(column[0], column[1]) for column in schema],
            [
                ("symbol", "VARCHAR"),
                ("trade_date", "DATE"),
                ("open", "DOUBLE"),
                ("high", "DOUBLE"),
                ("low", "DOUBLE"),
                ("close", "DOUBLE"),
                ("volume_shares", "BIGINT"),
                ("amount", "DOUBLE"),
                ("currency", "VARCHAR"),
                ("source", "VARCHAR"),
                ("adjustment", "VARCHAR"),
                ("fetched_at", "TIMESTAMP WITH TIME ZONE"),
            ],
        )

        first_fetched_at = rows[0][11]
        self.assertIsNotNone(first_fetched_at)
        for row, expected in zip(rows, expected_rows):
            self.assertEqual(row[0], expected["symbol"])
            self.assertEqual(row[1], date.fromisoformat(expected["trade_date"]))
            for index, column in enumerate(("open", "high", "low", "close"), start=2):
                self.assertAlmostEqual(row[index], expected[column])
            self.assertEqual(row[6], expected["volume_shares"])
            self.assertAlmostEqual(row[7], expected["amount"])
            self.assertEqual(row[8], expected["currency"])
            self.assertEqual(row[9], expected["source"])
            self.assertEqual(row[10], expected["adjustment"])
            self.assertEqual(row[11], first_fetched_at)

    def test_fetched_at_is_after_full_response(self):
        """The batch timestamp is assigned after the delayed response is complete."""
        self.server.response_mode = "slow_body"
        con = self.get_connection()
        row = con.execute(
            "SELECT fetched_at FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        ).fetchone()

        self.assertIsNotNone(self.server.response_completed_at)
        self.assertGreaterEqual(row[0].timestamp(), self.server.response_completed_at)


    def test_date_filtering_subrange(self):
        """Verify query returns only requested subrange within upstream kline response."""
        fixture_path = FIXTURES_DIR / "stock_daily.json"
        with open(fixture_path, "r", encoding="utf-8") as f:
            self.server.custom_body = f.read()
        self.server.response_mode = "custom"

        con = self.get_connection()
        rows = con.execute(
            "SELECT trade_date FROM aki_stock_daily('sz000001', DATE '2024-01-03', DATE '2024-01-04', source='eastmoney') "
            "ORDER BY trade_date ASC;"
        ).fetchall()

        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[0][0], date(2024, 1, 3))
        self.assertEqual(rows[1][0], date(2024, 1, 4))


class TestUserStory2ErrorClassification(TestDuckAkiBase):
    """User Story 2: SC-004 error taxonomy."""

    def test_invalid_argument_symbol(self):
        """Unsupported symbol returns DUCKAKI_INVALID_ARGUMENT."""
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('600519', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_INVALID_ARGUMENT", str(ctx.exception))

    def test_invalid_argument_source(self):
        """Unsupported source returns DUCKAKI_INVALID_ARGUMENT."""
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='unsupported');"
            ).fetchall()
        self.assertIn("DUCKAKI_INVALID_ARGUMENT", str(ctx.exception))

    def test_invalid_argument_inverted_dates(self):
        """Inverted dates return DUCKAKI_INVALID_ARGUMENT."""
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-10', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_INVALID_ARGUMENT", str(ctx.exception))

    def test_limit_exceeded_date_range(self):
        """Date range > 366 days returns DUCKAKI_LIMIT_EXCEEDED."""
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2023-01-01', DATE '2024-02-01', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_LIMIT_EXCEEDED", str(ctx.exception))
    def test_limit_exceeded_response_headers(self):
        """Response headers over 32 KiB use DUCKAKI_LIMIT_EXCEEDED."""
        self.server.response_mode = "large_headers"
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        message = str(ctx.exception)
        self.assertIn("DUCKAKI_LIMIT_EXCEEDED", message)
        self.assertNotIn("DUCKAKI_SOURCE_ACCESS", message)

    def test_source_access_tls_failure(self):
        """A loopback plaintext response on an HTTPS endpoint fails during TLS."""
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        port = listener.getsockname()[1]

        def send_plaintext_to_tls_client():
            connection, _ = listener.accept()
            with connection:
                connection.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
            listener.close()

        server_thread = threading.Thread(target=send_plaintext_to_tls_client, daemon=True)
        server_thread.start()
        previous_endpoint = os.environ["DUCKAKI_TEST_ENDPOINT"]
        os.environ["DUCKAKI_TEST_ENDPOINT"] = f"https://127.0.0.1:{port}/kline"
        try:
            con = self.get_connection()
            with self.assertRaises(Exception) as ctx:
                con.execute(
                    "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
                ).fetchall()
            message = str(ctx.exception)
            self.assertIn("DUCKAKI_SOURCE_ACCESS", message)
            self.assertNotIn("DUCKAKI_SOURCE_DATA", message)
        finally:
            os.environ["DUCKAKI_TEST_ENDPOINT"] = previous_endpoint
            listener.close()
            server_thread.join(timeout=2.0)

    def test_slow_drip_respects_short_test_deadline(self):
        """A response that keeps sending bytes still respects the monotonic deadline."""
        self.server.response_mode = "slow_drip"
        os.environ["DUCKAKI_TEST_TIMEOUT_MS"] = "120"
        con = self.get_connection()
        started = time.perf_counter()
        try:
            with self.assertRaises(Exception) as ctx:
                con.execute(
                    "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
                ).fetchall()
            elapsed = time.perf_counter() - started
            self.assertIn("DUCKAKI_SOURCE_ACCESS", str(ctx.exception))
            self.assertLess(elapsed, 1.0)
            self.assertEqual(self.server.request_count, 1)
        finally:
            os.environ["DUCKAKI_TEST_TIMEOUT_MS"] = "30000"

    def test_source_access_http_500(self):
        """HTTP 500 from upstream returns DUCKAKI_SOURCE_ACCESS."""
        self.server.response_mode = "500"
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_SOURCE_ACCESS", str(ctx.exception))

    def test_source_data_invalid_json(self):
        """Invalid JSON from upstream returns DUCKAKI_SOURCE_DATA."""
        self.server.response_mode = "invalid_json"
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_SOURCE_DATA", str(ctx.exception))

    def test_limit_exceeded_body_size(self):
        """Body > 1 MiB returns DUCKAKI_LIMIT_EXCEEDED."""
        self.server.response_mode = "large_body"
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_LIMIT_EXCEEDED", str(ctx.exception))
    def test_source_access_http_403_and_no_retry(self):
        """HTTP 403 returns DUCKAKI_SOURCE_ACCESS with strictly 1 request (no retry)."""
        self.server.response_mode = "403"
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_SOURCE_ACCESS", str(ctx.exception))
        self.assertEqual(self.server.request_count, 1)

    def test_source_access_http_429(self):
        """HTTP 429 returns DUCKAKI_SOURCE_ACCESS."""
        self.server.response_mode = "429"
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_SOURCE_ACCESS", str(ctx.exception))

    def test_source_access_redirect_blocked(self):
        """HTTP 302 redirect is not followed and returns DUCKAKI_SOURCE_ACCESS."""
        self.server.response_mode = "302"
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_SOURCE_ACCESS", str(ctx.exception))

    def test_source_data_truncated(self):
        """Truncated upstream response returns DUCKAKI_SOURCE_DATA."""
        self.server.response_mode = "truncated"
        con = self.get_connection()
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            ).fetchall()
        self.assertIn("DUCKAKI_SOURCE_DATA", str(ctx.exception))


class TestUserStory3Datasets(TestDuckAkiBase):
    """User Story 3: Offline capability catalog inspection."""

    def test_datasets_five_consecutive_runs_timing_and_zero_requests(self):
        """Query aki_datasets() 5 consecutive times, each < 2s, 0 network requests."""
        con = self.get_connection()
        timings = []
        for _ in range(5):
            t0 = time.perf_counter()
            rows = con.execute("SELECT dataset, function_name, source, description FROM aki_datasets();").fetchall()
            elapsed = time.perf_counter() - t0
            timings.append(elapsed)
            self.assertLess(elapsed, 2.0, f"Query took {elapsed}s >= 2.0s")
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0][0], "stock_daily")
            self.assertEqual(rows[0][1], "aki_stock_daily")
            self.assertEqual(rows[0][2], "eastmoney")
            self.assertIn("sz000001", rows[0][3])
            self.assertIn("不复权", rows[0][3])
            self.assertIn("366", rows[0][3])

        # Strictly 0 network requests
        self.assertEqual(self.server.request_count, 0)

    def test_datasets_works_with_external_access_disabled(self):
        """Catalog query succeeds even when enable_external_access=false."""
        con = self.get_connection()
        con.execute("SET enable_external_access=false;")
        rows = con.execute("SELECT * FROM aki_datasets();").fetchall()
        self.assertEqual(len(rows), 1)
        self.assertEqual(self.server.request_count, 0)


class TestUserStory4SQLComposition(TestDuckAkiBase):
    """User Story 4: SQL composition with JOIN, aggregation, and CTEs."""

    def test_join_with_watchlist(self):
        """JOIN the complete known fixture against a local watchlist."""
        fixture_path = FIXTURES_DIR / "stock_daily.json"
        self.server.custom_body = fixture_path.read_text(encoding="utf-8")
        self.server.response_mode = "custom"
        con = self.get_connection()
        con.execute("CREATE TEMP TABLE watchlist (symbol VARCHAR, target_price DOUBLE);")
        con.execute("INSERT INTO watchlist VALUES ('sz000001', 10.0);")

        rows = con.execute(
            "SELECT w.symbol, d.trade_date, d.close, w.target_price, (w.target_price - d.close) AS gap "
            "FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney') d "
            "JOIN watchlist w ON d.symbol = w.symbol ORDER BY d.trade_date;"
        ).fetchall()

        expected_dates = [date(2024, 1, 2), date(2024, 1, 3), date(2024, 1, 4), date(2024, 1, 5)]
        expected_closes = [9.15, 9.18, 9.10, 9.25]
        expected_gaps = [0.85, 0.82, 0.90, 0.75]
        self.assertEqual(len(rows), 4)
        for row, trade_date, close, gap in zip(rows, expected_dates, expected_closes, expected_gaps):
            self.assertEqual(row[0], "sz000001")
            self.assertEqual(row[1], trade_date)
            self.assertAlmostEqual(row[2], close)
            self.assertAlmostEqual(row[3], 10.0)
            self.assertAlmostEqual(row[4], gap)

    def test_aggregation_and_window(self):
        """Monthly aggregation matches the four-row known fixture."""
        self.server.custom_body = (FIXTURES_DIR / "stock_daily.json").read_text(encoding="utf-8")
        self.server.response_mode = "custom"
        con = self.get_connection()
        rows = con.execute(
            "SELECT symbol, date_trunc('month', trade_date)::DATE AS month, currency, "
            "avg(close), max(high), sum(amount) "
            "FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney') "
            "GROUP BY symbol, month, currency;"
        ).fetchall()

        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0][0], "sz000001")
        self.assertEqual(rows[0][1], date(2024, 1, 1))
        self.assertEqual(rows[0][2], "CNY")
        self.assertAlmostEqual(rows[0][3], 9.17)
        self.assertAlmostEqual(rows[0][4], 9.28)
        self.assertAlmostEqual(rows[0][5], 106400001.45)

    def test_ctas_snapshot_offline_repeated_queries(self):
        """A known CTAS snapshot supports three offline reads without another request."""
        self.server.custom_body = (FIXTURES_DIR / "stock_daily.json").read_text(encoding="utf-8")
        self.server.response_mode = "custom"
        con = self.get_connection()
        con.execute(
            "CREATE TEMP TABLE daily_temp AS "
            "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        )
        self.assertEqual(self.server.request_count, 1)
        self.server.response_mode = "500"

        for _ in range(3):
            rows = con.execute(
                "SELECT count(*), avg(close), sum(amount) FROM daily_temp;"
            ).fetchall()
            self.assertEqual(rows[0][0], 4)
            self.assertAlmostEqual(rows[0][1], 9.17)
            self.assertAlmostEqual(rows[0][2], 106400001.45)

        self.assertEqual(self.server.request_count, 1)



    def test_read_only_query_does_not_create_tables_or_cache(self):
        """Executing aki_stock_daily creates zero tables or cache objects in DuckDB catalog."""
        con = self.get_connection()
        tables_before = con.execute(
            "SELECT table_name FROM information_schema.tables WHERE table_schema NOT IN ('pg_catalog', 'information_schema');"
        ).fetchall()

        # Execute read
        con.execute(
            "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        ).fetchall()

        tables_after = con.execute(
            "SELECT table_name FROM information_schema.tables WHERE table_schema NOT IN ('pg_catalog', 'information_schema');"
        ).fetchall()
        self.assertEqual(tables_before, tables_after)


class TestUserStory5Persistence(TestDuckAkiBase):
    """User Story 5: CTAS, Parquet export, and persistence."""

    def test_ctas_and_parquet_export(self):
        import tempfile

        fixture_body = (FIXTURES_DIR / "stock_daily.json").read_text(encoding="utf-8")
        expected_rows = json.loads((FIXTURES_DIR / "expected_daily.json").read_text(encoding="utf-8"))
        expected_schema = [
            ("symbol", "VARCHAR"),
            ("trade_date", "DATE"),
            ("open", "DOUBLE"),
            ("high", "DOUBLE"),
            ("low", "DOUBLE"),
            ("close", "DOUBLE"),
            ("volume_shares", "BIGINT"),
            ("amount", "DOUBLE"),
            ("currency", "VARCHAR"),
            ("source", "VARCHAR"),
            ("adjustment", "VARCHAR"),
            ("fetched_at", "TIMESTAMP WITH TIME ZONE"),
        ]

        with tempfile.TemporaryDirectory() as td:
            db_path = os.path.join(td, "test_persistent.db")
            parquet_path = os.path.join(td, "stock_daily.parquet")
            self.server.custom_body = fixture_body
            self.server.response_mode = "custom"

            con = duckdb.connect(db_path, config={"allow_unsigned_extensions": "true"})
            con.execute(f"LOAD '{self.ext_path}';")
            con.execute(
                "CREATE TABLE stock_archive AS "
                "SELECT symbol, trade_date, open, high, low, close, "
                "CASE WHEN trade_date = DATE '2024-01-03' THEN NULL ELSE volume_shares END AS volume_shares, "
                "CASE WHEN trade_date = DATE '2024-01-03' THEN NULL ELSE amount END AS amount, "
                "currency, source, adjustment, fetched_at "
                "FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            )
            baseline_rows = con.execute(
                "SELECT * FROM stock_archive ORDER BY trade_date;"
            ).fetchall()
            self.assertEqual(len(baseline_rows), 4)
            table_schema = con.execute("DESCRIBE stock_archive;").fetchall()
            self.assertEqual([(column[0], column[1]) for column in table_schema], expected_schema)

            first_fetched_at = baseline_rows[0][11]
            for row, expected in zip(baseline_rows, expected_rows):
                expected_date = date.fromisoformat(expected["trade_date"])
                self.assertEqual(row[0], expected["symbol"])
                self.assertEqual(row[1], expected_date)
                for index, column in enumerate(("open", "high", "low", "close"), start=2):
                    self.assertAlmostEqual(row[index], expected[column])
                if expected_date == date(2024, 1, 3):
                    self.assertIsNone(row[6])
                    self.assertIsNone(row[7])
                else:
                    self.assertEqual(row[6], expected["volume_shares"])
                    self.assertAlmostEqual(row[7], expected["amount"])
                self.assertEqual(row[8], expected["currency"])
                self.assertEqual(row[9], expected["source"])
                self.assertEqual(row[10], expected["adjustment"])
                self.assertEqual(row[11], first_fetched_at)

            con.execute(f"COPY stock_archive TO '{parquet_path}' (FORMAT PARQUET);")
            con.close()

            # A later upstream revision must not mutate either saved representation.
            revised_payload = json.loads(fixture_body)
            revised_fields = revised_payload["data"]["klines"][0].split(",")
            revised_fields[2] = "8.88"
            revised_fields[5] = "99999"
            revised_fields[6] = "123456.78"
            revised_payload["data"]["klines"][0] = ",".join(revised_fields)
            self.server.custom_body = json.dumps(revised_payload)
            revision_con = self.get_connection()
            revised_row = revision_con.execute(
                "SELECT trade_date, close, volume_shares, amount "
                "FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney') "
                "WHERE trade_date = DATE '2024-01-02';"
            ).fetchone()
            self.assertEqual(revised_row[0], date(2024, 1, 2))
            self.assertAlmostEqual(revised_row[1], 8.88)
            self.assertEqual(revised_row[2], 9999900)
            self.assertAlmostEqual(revised_row[3], 123456.78)
            revision_con.close()
            self.assertEqual(self.server.request_count, 2)

            offline = duckdb.connect(db_path, config={"allow_unsigned_extensions": "true"})
            offline.execute("SET enable_external_access=false;")
            archived_rows = offline.execute(
                "SELECT * FROM stock_archive ORDER BY trade_date;"
            ).fetchall()
            archived_schema = offline.execute("DESCRIBE stock_archive;").fetchall()
            self.assertEqual(archived_rows, baseline_rows)
            self.assertEqual([(column[0], column[1]) for column in archived_schema], expected_schema)
            offline.close()

            parquet = duckdb.connect(config={"allow_unsigned_extensions": "true"})
            # The local Parquet file needs DuckDB filesystem access; this session does not load DuckAki.
            parquet_rows = parquet.execute(
                f"SELECT * FROM read_parquet('{parquet_path}') ORDER BY trade_date;"
            ).fetchall()
            parquet_schema = parquet.execute(
                f"DESCRIBE SELECT * FROM read_parquet('{parquet_path}');"
            ).fetchall()
            self.assertEqual(parquet_rows, baseline_rows)
            self.assertEqual([(column[0], column[1]) for column in parquet_schema], expected_schema)
            parquet.close()
            self.assertEqual(self.server.request_count, 2)

    def test_ctas_fail_if_exists_and_replace(self):
        """Verify DuckDB standard overwrite and fail-if-exists semantics on CTAS."""
        self.server.custom_body = (FIXTURES_DIR / "stock_daily.json").read_text(encoding="utf-8")
        self.server.response_mode = "custom"
        con = self.get_connection()
        con.execute(
            "CREATE TEMP TABLE target_tbl AS "
            "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        )

        con.execute(
            "UPDATE target_tbl SET close = 42.0 WHERE trade_date = DATE '2024-01-02';"
        )
        # Repeat CTAS without OR REPLACE must fail
        with self.assertRaises(Exception) as ctx:
            con.execute(
                "CREATE TEMP TABLE target_tbl AS "
                "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
            )
        self.assertIn("already exists", str(ctx.exception).lower())
        stored_close = con.execute(
            "SELECT close FROM target_tbl WHERE trade_date = DATE '2024-01-02';"
        ).fetchone()[0]
        self.assertEqual(stored_close, 42.0)
        self.assertEqual(self.server.request_count, 1)

        # CTAS with OR REPLACE succeeds
        con.execute(
            "CREATE OR REPLACE TEMP TABLE target_tbl AS "
            "SELECT * FROM aki_stock_daily('sz000001', DATE '2024-01-01', DATE '2024-01-05', source='eastmoney');"
        )
        count = con.execute("SELECT count(*) FROM target_tbl;").fetchone()[0]
        self.assertEqual(count, 4)
        self.assertEqual(self.server.request_count, 2)


if __name__ == "__main__":
    unittest.main()
