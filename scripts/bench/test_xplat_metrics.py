import unittest
from xplat_metrics import cpu_delta


class MetricsTests(unittest.TestCase):
    def test_cpu_handles_new_helpers_and_reused_pids(self):
        before = {'processes': [dict(pid=1, born=10, cpu_s=4), dict(pid=2, born=10, cpu_s=8)]}
        after = {'processes': [dict(pid=1, born=10, cpu_s=5), dict(pid=2, born=20, cpu_s=2)]}
        self.assertEqual(cpu_delta(before, after), 3)



class AttributionTests(unittest.TestCase):
    def test_macos_adds_only_new_webkit_identities(self):
        import sys
        from unittest.mock import Mock, patch
        from xplat_metrics import tree
        def process(pid, name):
            p = Mock(pid=pid)
            p.create_time.return_value = 10
            p.name.return_value = name
            p.cmdline.return_value = [name]
            return p
        root = process(1, 'app')
        child = process(2, 'child')
        root.children.return_value = [child]
        old = process(3, 'com.apple.WebKit.WebContent')
        new = process(4, 'com.apple.WebKit.Networking')
        unrelated = process(5, 'other')
        fake = Mock()
        fake.process_iter.return_value = [old, new, unrelated]
        with patch.dict(sys.modules, {'psutil': fake}), patch('xplat_metrics.sys.platform', 'darwin'):
            self.assertEqual([p.pid for p in tree(root, {(3, 10)})], [1, 2, 4])
        unrelated.cmdline.assert_not_called()


if __name__ == '__main__':
    unittest.main()
