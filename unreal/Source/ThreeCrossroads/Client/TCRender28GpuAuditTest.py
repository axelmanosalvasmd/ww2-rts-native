"""Deterministic tests for the offline diagnostic, never an engine/GPU run.

Run with TC_RENDER28_TEST_SCRATCH set to a task-owned scratch directory.
"""
import importlib.util
import json
import os
from pathlib import Path
import struct
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location('render28_gpu', Path(__file__).with_name('TCRender28GpuAudit.py'))
GPU = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GPU)


class GpuAuditTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=os.environ['TC_RENDER28_TEST_SCRATCH'])
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'Base').mkdir()
        (self.root / 'Resources').mkdir()
        self.resource = '00010000000000000100'
        self.subresource = self.resource + '.mip0'
        self.desc = dict(Name='SceneDepthZ', UniqueResourceName=self.resource, Desc='FRDGTextureDesc',
                         Type='Texture2D', Format='PF_DepthStencil', ExtentX=5, ExtentY=5,
                         Depth=1, ArraySize=1, NumMips=1, NumSamples=1)
        self.passes = [dict(Id=i, Pointer=f'0001{i:016x}', EventName=name, ParentEventScopes=['Frame 4'],
                            InputResources=inputs, OutputResources=outputs)
                       for i, name, inputs, outputs in [
                           (1, 'PrePass', [], [self.subresource]),
                           (2, 'VelocityParallel', [], [self.subresource]),
                           (3, 'BasePassParallel', [self.subresource], [])]]
        self.binary = self.root / 'Resources' / (self.subresource + '.v' + self.passes[1]['Pointer'] + '.bin')
        self.binary.write_bytes(struct.pack('<25f', *[i / 32 for i in range(25)]))
        self.write_graph()

    def write_graph(self):
        (self.root / 'Base/Status.txt').write_text('ok')
        (self.root / 'Base/Passes.json').write_text('\n'.join(json.dumps(p) for p in self.passes))
        (self.root / 'Base/ResourceDescs.json').write_text(json.dumps(self.desc))

    def sample(self, **kwargs):
        return GPU.sample(self.root, self.subresource, self.passes[-1]['Pointer'], 2, 2, 'top-left', **kwargs)

    def test_exact_last_writer_and_raw_version_are_required(self):
        result = self.sample()
        self.assertEqual(result['writer']['Pointer'], self.passes[1]['Pointer'])
        self.assertEqual(result['reader']['Pointer'], self.passes[2]['Pointer'])
        self.assertEqual(result['samples'][4]['rawHex'], struct.pack('<f', 12 / 32).hex())
        self.assertEqual(result['samples'][4]['byteOffset'], 48)
        # An earlier captured prepass must never replace an uncaptured later writer.
        self.binary.rename(self.binary.with_name(self.subresource + '.v' + self.passes[0]['Pointer'] + '.bin'))
        with self.assertRaisesRegex(ValueError, 'last writer binary'):
            self.sample()
        self.passes[-1]['InputResources'] = [self.resource + '.stencil']
        self.write_graph()
        with self.assertRaisesRegex(ValueError, 'consume'):
            self.sample()

    def test_layout_and_complete_capture_are_not_guessed(self):
        self.assertEqual(self.sample()['readbackFormat'], 'PF_R32_FLOAT')
        flipped = GPU.sample(self.root, self.subresource, self.passes[-1]['Pointer'], 2, 1, 'bottom-left')
        self.assertEqual(flipped['samples'][4]['byteOffset'], 68)
        self.assertEqual(flipped['samples'][4]['values'], [17 / 32])
        self.binary.write_bytes(self.binary.read_bytes()[:-1])
        with self.assertRaisesRegex(ValueError, 'byte count'):
            self.sample()
        for field, value in [('NumSamples', 4), ('Format', 'PF_Unknown'), ('Type', 'TextureCube')]:
            before = self.desc[field]
            self.desc[field] = value
            self.write_graph()
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.sample()
            self.desc[field] = before
        self.write_graph()
        (self.root / 'Base/Status.txt').write_text('dumping')
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            self.sample()

    def test_exact_runtime_role_and_transition_pointers(self):
        for receipt in ({'exit': 0}, {'exits': {'native': {'exit': 0}, 'source': {'exit': -11}}},
                        {'native': {'exit': 0}, 'source': {'exit': -11}}):
            self.assertEqual(GPU.role_exit(receipt, 'native'), 0)
        for receipt in ({'exits': {'source': {'exit': 0}}}, {'native': {'exit': -11}, 'source': {'exit': 0}},
                        {'exit': False}, {'exits': {'native': {'timeout': True, 'exit': 0}}},
                        {'timeout': True, 'exits': {'native': {'exit': 0}}}):
            with self.subTest(receipt=receipt), self.assertRaises(ValueError):
                GPU.require_clean_exit(receipt, 'native')
        log = self.root / 'native.log'
        log.write_text('[a][ 4]LogRDG: Display: [ClearBuffer(NumCulledLightsGrid Size=122880bytes)(Index: 33, Pipeline: Graphics): Batch: (0x88) Test] (Begin):\n'
                       '[a][ 4]LogRDG: Display: \\tRDG(0x100) RHI(0x200) NumCulledLightsGrid: [Unknown, Graphics] -> [UAVCompute, Graphics]\n'
                       '[a][ 4]LogRDG: Display: \\tRDG(0x300) RHI(0x400) SceneDepthZ:\n'
                       '[a][ 4]LogRDG: Display: \\t\\tMip(0), Array(0), Slice(1): [DSVWrite, Graphics] -> [SRVGraphics, Graphics]\n'
                       '[a][ 5]LogRDG: Display: \\tRDG(0x500) RHI(0x600) UnboundOtherFrame:\n')
        result = GPU.transitions(log)
        self.assertEqual(result['links'][0]['rdgPointer'], '0x100')
        self.assertEqual(result['links'][0]['rhiPointer'], '0x200')
        self.assertEqual(result['links'][0]['passIndex'], 33)
        self.assertEqual(result['links'][0]['logFrame'], 4)
        self.assertEqual(len(result['links']), 2)
        self.assertEqual(result['links'][1]['subresourceTransitions'][0]['planeSlice'], 1)
        self.assertFalse(result['failedResourceIdentified'])

    def test_timestamped_qualified_pass_keeps_exact_name(self):
        log = self.root / 'qualified.log'
        name = 'Scene/ComputeLightGrid/ClearBuffer(NumCulledLightsGrid Size=69120bytes)'
        log.write_text('[2026.10.20-16.33.52:123][ 54]LogRDG: Display: [' + name + '(Index: 31, Pipeline: Graphics): Batch: (0x88) Epilogue (For Graphics)] (Begin):\n'
                       '[2026.10.20-16.33.52:123][ 54]LogRDG: Display: RDG(0x100) RHI(0x200) NumCulledLightsGrid: [UAVCompute, Graphics] -> [UAVCompute, Graphics]\n'
                       '[2026.10.20-16.33.52:124][ 55]LogRDG: Display: RDG(0x300) RHI(0x400) OtherFrame: [Unknown, Graphics] -> [UAVCompute, Graphics]\n')
        links = GPU.transitions(log)['links']
        self.assertEqual(len(links), 1)
        self.assertEqual(links[0]['passName'], name)
        self.assertEqual(links[0]['logFrame'], 54)
        self.assertEqual(links[0]['passIndex'], 31)
        self.assertEqual(links[0]['rdgPointer'], '0x100')
        self.assertEqual(links[0]['rhiPointer'], '0x200')


if __name__ == '__main__':
    unittest.main()
