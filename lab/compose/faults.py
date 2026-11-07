"""Run while the test MongoDB is stopped. Financial commits must still succeed."""
import sys
import unittest

from business import TARGETS, Contract


class MongoFailure:
    def test_money_succeeds_without_mongo(self):
        sender, a = self.user(50_000)
        receiver, b = self.user()
        self.call(sender, "POST", "/transactions/withdrawal", json={"amount": 10_000})
        self.call(sender, "POST", "/transfers", json={"recipient": b["phone"], "amount": 15_000})
        self.assertEqual(self.call(sender, "GET", "/accounts/me").json()["balance"], 25_000)
        self.assertEqual(self.call(receiver, "GET", "/accounts/me").json()["balance"], 15_000)
        self.call(sender, "POST", "/transfers", 409, "insufficient_funds", json={"recipient": b["phone"], "amount": 30_000})
        self.consistent(a, b)


suite = unittest.TestSuite()
for target in TARGETS:
    # Contract는 도우미만 빌린다. 정상 MongoDB가 필요한 테스트는 여기서 실행하지 않는다.
    helpers = {name: value for name, value in vars(Contract).items() if not name.startswith("test_") and not name.startswith("__")}
    case = type(target.title() + "MongoFailure", (MongoFailure, unittest.TestCase), {**helpers, "target": target})
    suite.addTests(unittest.defaultTestLoader.loadTestsFromTestCase(case))
result = unittest.TextTestRunner(verbosity=2).run(suite)
sys.exit(not result.wasSuccessful())
