import unittest
from xplat_metrics import cpu_delta


class MetricsTests(unittest.TestCase):
    def test_cpu_handles_new_helpers_and_reused_pids(self):
        before = {'processes': [dict(pid=1, born=10, cpu_s=4), dict(pid=2, born=10, cpu_s=8)]}
        after = {'processes': [dict(pid=1, born=10, cpu_s=5), dict(pid=2, born=20, cpu_s=2)]}
        self.assertEqual(cpu_delta(before, after), 3)


if __name__ == '__main__':
    unittest.main()
