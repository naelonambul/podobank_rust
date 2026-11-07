"""HTTP contract tests against each API and the real, Compose-only databases.

Never clears tables: every test uses newly registered accounts. Credentials stay
in the container environment sourced from lab/compose/.env and are never printed.
"""
import concurrent.futures
import hashlib
import os
import sys
import unittest
import uuid
from urllib.parse import urlsplit

import httpx
from pymongo import MongoClient
from sqlalchemy import text

from app.db.mariadb import make_engine

URL = os.environ["MARIADB_URL"]
if urlsplit(URL).hostname != "mariadb" or urlsplit(URL).path != "/podobank_test":
    raise SystemExit("Only the isolated Compose test database is supported")
ENGINE = make_engine(URL)
MONGO = MongoClient(os.environ["MONGODB_URL"])[os.environ["MONGODB_DB"]]
PASSWORD = urlsplit(URL).password
TARGETS = sys.argv[1:] or ["python", "rust"]
sys.argv = sys.argv[:1]


class Contract:
    def client(self, cookies=None):
        c = httpx.Client(base_url=f"http://{self.target}:8000/api/v1", cookies=cookies, timeout=20)
        self.addCleanup(c.close)
        return c

    def call(self, c, method, path, status=200, code=None, **kwargs):
        r = c.request(method, path, **kwargs)
        self.assertEqual(r.status_code, status, f"{method} {path}: {r.status_code}, expected {status}")
        uuid.UUID(r.headers["x-request-id"])
        if code:
            self.assertEqual(r.json()["code"], code)
            self.assertEqual(r.json()["request_id"], r.headers["x-request-id"])
        return r

    def user(self, balance=0):
        c = self.client()
        phone = "010" + str(uuid.uuid4().int % 100_000_000).zfill(8)
        body = {"name": "비교고객", "phone_number": phone, "password": PASSWORD, "birth_year": 1995,
                "gender": "female", "marital_status": "single", "is_employed": True}
        r = self.call(c, "POST", "/auth/signup", json=body)
        self.assertEqual(r.json(), {"status": "authenticated"})
        me = self.call(c, "GET", "/auth/me").json()
        account = self.call(c, "GET", "/accounts/me").json()
        if balance:
            for amount in range(0, balance, 10_000_000):
                self.call(c, "POST", "/transactions/deposit", json={"amount": min(balance - amount, 10_000_000)})
        return c, {**me, **account, "phone": phone, "body": body}

    def sql(self, statement, **params):
        with ENGINE.begin() as conn:
            return conn.execute(text(statement), params).mappings().all() if statement.lstrip().upper().startswith("SELECT") else conn.execute(text(statement), params).rowcount

    def consistent(self, *users):
        for u in users:
            [row] = self.sql("SELECT a.balance, COALESCE(SUM(l.amount),0) AS total FROM accounts a LEFT JOIN ledger_entries l ON l.account_id=a.id WHERE a.user_id=:u GROUP BY a.id", u=u["id"])
            self.assertGreaterEqual(row["balance"], 0)
            self.assertEqual(row["balance"], row["total"])

    def test_authentication(self):
        self.call(self.client(), "GET", "/auth/me", 401, "authentication_required")
        c, u = self.user()
        token = c.cookies["podobank_session"]
        [s] = self.sql("SELECT token_hash,device_id,client FROM sessions WHERE user_id=:u AND ended_at IS NULL", u=u["id"])
        self.assertEqual(s["token_hash"], hashlib.sha256(token.encode()).hexdigest())
        uuid.UUID(s["device_id"])
        self.assertEqual(s["client"], "web")
        self.call(c, "POST", "/auth/logout")
        self.call(self.client({"podobank_session": token}), "GET", "/auth/me", 401, "authentication_required")
        r = self.call(c, "POST", "/auth/login", json={"phone_number": u["phone"], "password": PASSWORD})
        cookies = ";".join(r.headers.get_list("set-cookie")).lower()
        for flag in ("httponly", "samesite=lax", "max-age=3600", "podobank_device="):
            self.assertIn(flag, cookies)
        self.call(c, "GET", "/auth/me")
        old = c.cookies["podobank_session"]
        self.call(c, "POST", "/auth/login", json={"phone_number": u["phone"], "password": PASSWORD})
        self.call(self.client({"podobank_session": old}), "GET", "/auth/me", 401, "authentication_required")

    def test_login_lock_expiry_and_duplicate_signup(self):
        c, u = self.user()
        self.call(c, "POST", "/auth/signup", 409, "phone_already_registered", json=u["body"])
        for _ in range(5):
            self.call(c, "POST", "/auth/login", 401, "invalid_credentials", json={"phone_number": u["phone"], "password": "invalid-example"})
        self.call(c, "POST", "/auth/login", 423, "account_locked", json={"phone_number": u["phone"], "password": PASSWORD})
        self.sql("UPDATE users SET locked_until=NOW()-INTERVAL 1 SECOND WHERE id=:u", u=u["id"])
        self.call(c, "POST", "/auth/login", json={"phone_number": u["phone"], "password": PASSWORD})

    def test_session_expiry_and_roles(self):
        c, u = self.user()
        self.sql("UPDATE sessions SET last_seen_at=NOW()-INTERVAL 11 MINUTE WHERE user_id=:u", u=u["id"])
        self.call(c, "GET", "/auth/me", 401, "authentication_required")
        self.call(c, "POST", "/auth/login", json={"phone_number": u["phone"], "password": PASSWORD})
        self.sql("UPDATE sessions SET created_at=NOW()-INTERVAL 61 MINUTE WHERE user_id=:u AND ended_at IS NULL", u=u["id"])
        self.call(c, "GET", "/auth/me", 401, "authentication_required")
        self.call(c, "POST", "/auth/login", json={"phone_number": u["phone"], "password": PASSWORD})
        self.sql("UPDATE users SET role='bank_employee' WHERE id=:u", u=u["id"])
        self.call(c, "GET", "/accounts/me", 403, "forbidden")
        self.call(c, "POST", "/transfers", 403, "forbidden", json={"recipient": u["account_number"], "amount": 1})

    def test_cross_language_passwords_and_sessions(self):
        c, u = self.user()
        other = "rust" if self.target == "python" else "python"
        with httpx.Client(base_url=f"http://{other}:8000/api/v1", timeout=20) as peer:
            header = "; ".join(f"{name}={value}" for name, value in c.cookies.items())
            self.assertEqual(peer.get("/auth/me", headers={"Cookie": header}).json()["id"], u["id"])
            self.assertEqual(peer.post("/auth/login", json={"phone_number": u["phone"], "password": PASSWORD}).status_code, 200)
            self.assertEqual(peer.get("/accounts/me").json()["account_number"], u["account_number"])
            header = "; ".join(f"{name}={value}" for name, value in peer.cookies.items())
            self.call(c, "GET", "/auth/me", headers={"Cookie": header})

    def test_lock_timeout_rolls_back(self):
        c, a = self.user(10_000)
        _, b = self.user()
        with ENGINE.begin() as conn:
            conn.execute(text("SELECT id FROM accounts WHERE user_id=:u FOR UPDATE"), {"u": a["id"]})
            self.call(c, "POST", "/transfers", 409, "lock_timeout", json={"recipient": b["phone"], "amount": 1_000})
        self.assertEqual(self.call(c, "GET", "/accounts/me").json()["balance"], 10_000)
        self.consistent(a, b)

    def test_failed_transfer_changes_nothing(self):
        c, a = self.user(10_000)
        receiver, b = self.user()
        r = self.call(c, "POST", "/transfers", 409, "insufficient_funds", json={"recipient": b["phone"], "amount": 10_001})
        self.assertEqual(self.call(receiver, "GET", "/accounts/me").json()["balance"], 0)
        self.assertEqual(MONGO.events_bronze.count_documents({"request_id": r.headers["x-request-id"], "outcome": "failure", "reason": "insufficient_funds"}), 1)
        self.consistent(a, b)

    def test_cash_ledger_and_validation(self):
        c, u = self.user(50_000)
        r = self.call(c, "POST", "/transactions/withdrawal", json={"amount": 10_000})
        self.assertEqual(r.json()["balance"], 40_000)
        self.call(c, "POST", "/transactions/withdrawal", 409, "insufficient_funds", json={"amount": 40_001})
        for value, code in ((0, "invalid_amount"), (-1, "invalid_amount"), (10_000_001, "amount_limit_exceeded"), (True, "validation_error"), ("1", "validation_error"), (1.5, "validation_error")):
            self.call(c, "POST", "/transactions/deposit", 422, code, json={"amount": value})
        self.call(c, "GET", "/accounts/me/ledger?limit=101", 422, "validation_error")
        rows = self.call(c, "GET", "/accounts/me/ledger?limit=10").json()["items"]
        self.assertEqual([r["entry_type"] for r in rows], ["withdrawal", "deposit"])
        self.assertTrue(rows[0]["created_at"].endswith("+09:00"))
        from datetime import UTC, datetime
        self.assertLess(abs((datetime.now(UTC) - datetime.fromisoformat(rows[0]["created_at"])).total_seconds()), 60)
        self.consistent(u)

    def test_validation_fields_and_profile_coercion(self):
        c = self.client()
        r = self.call(c, "POST", "/auth/login", 422, "validation_error", json={})
        self.assertEqual(set(r.json()["fields"]), {"phone_number", "password"})
        r = self.call(c, "POST", "/auth/signup", 422, "validation_error", json={})
        self.assertEqual(set(r.json()["fields"]), {"name", "phone_number", "password", "birth_year", "gender", "marital_status", "is_employed"})
        c, u = self.user()
        r = self.call(c, "POST", "/transfers", 422, "validation_error", json={})
        self.assertEqual(set(r.json()["fields"]), {"recipient", "amount"})
        r = self.call(c, "GET", "/accounts/me/ledger?limit=0&offset=-1", 422, "validation_error")
        self.assertEqual(set(r.json()["fields"]), {"limit", "offset"})
        self.call(c, "GET", "/accounts/me/ledger?limit=20.0&offset=0")
        body = dict(u["body"], phone_number="010" + str(uuid.uuid4().int % 100_000_000).zfill(8), birth_year="1995", is_employed="yes")
        self.call(self.client(), "POST", "/auth/signup", json=body)

    def test_transfer_contract_and_events(self):
        c, a = self.user(100_000)
        _, b = self.user()
        for query in (b["phone"], b["account_number"]):
            r = self.call(c, "POST", "/transfers/recipient-lookup", json={"query": query})
            self.assertEqual(r.json(), {"name": b["name"]})
        self.call(c, "POST", "/transfers", 422, "self_transfer", json={"recipient": a["account_number"], "amount": 1})
        self.call(c, "POST", "/transfers", 404, "recipient_not_found", json={"recipient": "invalid-number", "amount": 1})
        r = self.call(c, "POST", "/transfers", json={"recipient": b["phone"], "amount": 30_000})
        txid = r.json()["transaction_id"]
        self.assertEqual(r.json()["balance"], 70_000)
        rows = self.sql("SELECT entry_type,amount FROM ledger_entries WHERE transaction_id=:t ORDER BY id", t=txid)
        self.assertEqual([(x["entry_type"], x["amount"]) for x in rows], [("transfer_out", -30_000), ("transfer_in", 30_000)])
        events = list(MONGO.events_bronze.find({"request_id": r.headers["x-request-id"]}, {"_id": 0}))
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]["outcome"], "success")
        forbidden = {"user_id", "account_id", "amount", "name", "phone_number", "account_number"}
        self.assertFalse(forbidden.intersection(events[0]))
        self.assertFalse(forbidden.intersection(events[0]["properties"]))
        self.consistent(a, b)

    def test_parallel_withdrawals(self):
        c, u = self.user(10_000)
        cookie = dict(c.cookies)
        def send(_):
            with httpx.Client(base_url=f"http://{self.target}:8000/api/v1", cookies=cookie, timeout=20) as cli:
                return cli.post("/transactions/withdrawal", json={"amount": 1_000}).status_code
        with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:
            statuses = list(pool.map(send, range(20)))
        self.assertEqual(statuses.count(200), 10)
        self.assertEqual(statuses.count(409), 10)
        self.assertEqual(self.call(c, "GET", "/accounts/me").json()["balance"], 0)
        self.consistent(u)

    def test_opposite_transfers(self):
        ca, a = self.user(100_000)
        cb, b = self.user(100_000)
        def send(i):
            source, receiver = (ca, b) if i % 2 else (cb, a)
            with httpx.Client(base_url=f"http://{self.target}:8000/api/v1", cookies=dict(source.cookies), timeout=20) as cli:
                return cli.post("/transfers", json={"recipient": receiver["account_number"], "amount": 1_000})
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            replies = list(pool.map(send, range(20)))
        self.assertTrue(all(r.status_code == 200 for r in replies))
        sent_a = sum(1_000 for i, r in enumerate(replies) if i % 2 and r.status_code == 200)
        sent_b = sum(1_000 for i, r in enumerate(replies) if not i % 2 and r.status_code == 200)
        self.assertEqual(self.call(ca, "GET", "/accounts/me").json()["balance"], 100_000 - sent_a + sent_b)
        self.assertEqual(self.call(cb, "GET", "/accounts/me").json()["balance"], 100_000 - sent_b + sent_a)
        for r in replies:
            [row] = self.sql("SELECT COUNT(*) AS n, SUM(amount) AS total FROM ledger_entries WHERE transaction_id=:t", t=r.json()["transaction_id"])
            self.assertEqual((row["n"], row["total"]), (2, 0))
        self.consistent(a, b)

    def test_many_parallel_transfers_match_success_count(self):
        c, a = self.user(100_000)
        receiver, b = self.user()
        def send(_):
            with httpx.Client(base_url=f"http://{self.target}:8000/api/v1", cookies=dict(c.cookies), timeout=20) as cli:
                return cli.post("/transfers", json={"recipient": b["phone"], "amount": 1_000})
        with concurrent.futures.ThreadPoolExecutor(max_workers=32) as pool:
            replies = list(pool.map(send, range(60)))
        self.assertTrue(all(r.status_code in {200, 409, 503} for r in replies))
        successful = [r for r in replies if r.status_code == 200]
        self.assertGreater(len(successful), 0)
        self.assertEqual(self.call(c, "GET", "/accounts/me").json()["balance"], 100_000 - len(successful) * 1_000)
        self.assertEqual(self.call(receiver, "GET", "/accounts/me").json()["balance"], len(successful) * 1_000)
        for r in successful:
            [row] = self.sql("SELECT COUNT(*) AS n, SUM(amount) AS total FROM ledger_entries WHERE transaction_id=:t", t=r.json()["transaction_id"])
            self.assertEqual((row["n"], row["total"]), (2, 0))
        self.consistent(a, b)

    def test_daily_limit_race(self):
        c, a = self.user(120_000_000)
        _, b = self.user()
        for _ in range(9):
            self.call(c, "POST", "/transfers", json={"recipient": b["account_number"], "amount": 10_000_000})
        def send(_):
            with httpx.Client(base_url=f"http://{self.target}:8000/api/v1", cookies=dict(c.cookies), timeout=20) as cli:
                return cli.post("/transfers", json={"recipient": b["account_number"], "amount": 8_000_000}).status_code
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            statuses = list(pool.map(send, range(2)))
        self.assertEqual(sorted(statuses), [200, 422])
        self.assertEqual(self.call(c, "GET", "/accounts/me").json()["balance"], 22_000_000)
        self.consistent(a, b)


for target in TARGETS:
    if target not in {"python", "rust"}:
        raise SystemExit("target must be python or rust")
    globals()[target.title() + "Contract"] = type(target.title() + "Contract", (Contract, unittest.TestCase), {"target": target})

if __name__ == "__main__":
    unittest.main(verbosity=2)
