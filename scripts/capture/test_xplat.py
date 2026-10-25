import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parent / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class XplatTests(unittest.TestCase):
    def test_comparison_retains_normalized_rmse_and_fuzz_pixels(self):
        module = load('xplat_report')
        with patch.object(module.subprocess, 'run') as run:
            run.side_effect = [type('Result', (), dict(returncode=1, stderr='655.35 (0.01)')),
                               type('Result', (), dict(returncode=1, stderr='42'))]
            result = module.compare(Path('a'), Path('b'), Path('diff'))
            self.assertEqual(result['rmse_normalized'], .01)
            self.assertEqual(result['different_pixels_2pct'], 42)
            self.assertIn('2%', run.call_args.args[0])

    def test_comparison_rejects_imagemagick_error(self):
        module = load('xplat_report')
        with patch.object(module.subprocess, 'run') as run:
            run.return_value = type('Result', (), dict(returncode=2, stderr='missing image'))
            with self.assertRaisesRegex(RuntimeError, 'missing image'):
                module.compare(Path('a'), Path('b'), Path('diff'))

    def test_stage_has_separate_identity_and_local_fonts(self):
        module = load('xplat_prepare')
        import json
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, content in {'src-tauri/tauri.conf.json': json.dumps(dict(identifier='production', app=dict(windows=[{}]), bundle={})),
                                  'src-tauri/capabilities/default.json': '{"permissions":[]}',
                                  'src/styles/fonts.css': 'remote',
                                  '.cache/capture/fonts/fonts.css': 'local'}.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
            with patch.object(module, 'ROOT', root), patch.object(module.subprocess, 'check_output', return_value=b'src-tauri/tauri.conf.json\0src-tauri/capabilities/default.json\0src/styles/fonts.css\0'):
                module.prepare(root / 'stage')
            config = json.loads((root / 'stage/src-tauri/tauri.conf.json').read_text())
            self.assertEqual(config['identifier'], 'com.blockfield.capture')
            self.assertFalse(config['bundle']['createUpdaterArtifacts'])
            self.assertEqual((root / 'stage/src/styles/fonts.css').read_text(), 'local')
            self.assertEqual((root / 'src/styles/fonts.css').read_text(), 'remote')


if __name__ == '__main__':
    unittest.main()
