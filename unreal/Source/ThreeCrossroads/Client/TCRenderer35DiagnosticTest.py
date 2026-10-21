"""Focused runner regressions. No engine or GPU process is started."""
import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch
import tempfile

HERE = Path(__file__).parent
module = HERE / os.environ.get('TC_RENDERER_TEST_MODULE', 'TCRenderer35Diagnostic.py')
spec = importlib.util.spec_from_file_location('diagnostic', module)
d = importlib.util.module_from_spec(spec)
spec.loader.exec_module(d)


class DiagnosticTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        d.SCRATCH.mkdir(parents=True, exist_ok=True)

    def test_validation_arguments_are_refused(self):
        for flag in ('-vulkandebug', '-VulkanDebugUtils', '-vulkanvalidation=1'):
            with self.subTest(flag=flag), self.assertRaises(ValueError):
                d.command(['/engine/UnrealEditor', '-game', flag], 'baseline', Path('/unused'), True)

    def test_environment_cannot_inherit_validation(self):
        env = d.environment({'VK_INSTANCE_LAYERS': 'VK_LAYER_KHRONOS_validation',
                             'VK_LOADER_LAYERS_ENABLE': '*validation*',
                             'VK_LAYER_PATH': '/old', 'MESA_VK_ABORT_ON_DEVICE_LOSS': '1'}, Path('/scratch'))
        self.assertNotIn('VK_INSTANCE_LAYERS', env)
        self.assertNotIn('VK_LOADER_LAYERS_ENABLE', env)
        self.assertNotIn('VK_LAYER_PATH', env)
        self.assertEqual(env['VK_LOADER_LAYERS_DISABLE'], '*')
        self.assertNotIn('MESA_VK_ABORT_ON_DEVICE_LOSS', env)

    def test_drm_deduplicates_clients_without_adding_memory_and_total(self):
        fields = {'drm-client-id': '7', 'drm-pdev': '0000:00:02.0',
                  'drm-memory-system': '1024 KiB', 'drm-total-system': '2048 KiB'}
        sample = d.parse_drm([fields, fields, fields | {'drm-client-id': '8'}])
        self.assertEqual(len(sample['clients']), 2)
        self.assertEqual(sample['bytes']['drm-memory-system'], 2 * 1024**2)
        self.assertEqual(sample['bytes']['drm-total-system'], 4 * 1024**2)
        self.assertIsNone(d.parse_drm([])['bytes'])

    def test_candidates_preserve_workload_and_disable_inventory_in_production(self):
        base = ['/engine/UnrealEditor', '-game', '-TCHostMap=three-islands', '-TCExitAfter=240']
        self.assertEqual(len(d.CASES), 6)
        for case in d.CASES:
            args = d.command(base, case, Path('/scratch/run'), True)
            self.assertIn('-TCExitAfter=240', args)
            self.assertIn('-TCHostMap=three-islands', args)
            self.assertNotIn('-TCRenderer35Memory', args)
            self.assertTrue(any(x == '-TCGraphicsAt=low@0' for x in args))
        self.assertIn('-TCRenderer35Memory', d.command(base, 'baseline', Path('/scratch/run'), False))

    def test_postmortem_collects_all_threads_and_removes_extracted_core(self):
        with tempfile.TemporaryDirectory(dir=d.SCRATCH) as work:
            out = Path(work)
            calls = []
            def run(args, **kwargs):
                calls.append(args)
                for arg in args:
                    if arg.startswith('--output='):
                        Path(arg.split('=', 1)[1]).write_bytes(b'fixture core')
                return type('Result', (), {'returncode': 0})()
            with patch.object(d.shutil, 'which', return_value='/fixture/tool'), patch.object(d.subprocess, 'run', side_effect=run):
                d.postmortem(123, '/fixture/UnrealEditor', out, out)
            self.assertIn('thread apply all bt full', calls[-1])
            self.assertFalse(list(out.glob('core-*/*')))
            self.assertIn('extracted core removed', (out / 'core-status.txt').read_text())


if __name__ == '__main__':
    unittest.main()
