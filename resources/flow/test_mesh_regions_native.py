"""Native regression for partition sweeps whose OCC return omits shared volumes.

Run explicitly with Gmsh available. VORTIA_REGION_CHECK_OPENFOAM=1 additionally
converts every exported mesh and checks all topology/geometry in OpenFOAM.
"""
import copy
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
if os.environ.get('VORTIA_GMSH_RUNTIME'):
    sys.path.insert(0, os.environ['VORTIA_GMSH_RUNTIME'])
from prepare import mesh_case
from boundaries import header


def shared_vertex_inputs():
    # Two non-orthogonal cuts meet at a rounded contour's vertex. Endpoints are
    # rounded to the six decimal places used by the coordinate editor (mm).
    sketch = dict(units='mm', closed=True, vertices=[
        [0, 0], [0, 100], [96.14204879243817, 100],
        [103.58114633948614, 98.5650162317598],
        [247.35263473895571, 40.95504092464176],
        [252.80866528914902, 40.38947151039958],
        [1144.4393236071173, 197.60801339870787],
        [1144.4393236071173, 2197.608013398708],
        [4644.439323607117, 2197.608013398708], [4644.439323607117, 0]],
        segmentBulges=[0, 0, -.09556789206848863, 0, .13981221321996284, 0, 0, 0, 0, 0])
    cfg = dict(mesh=dict(engine='occ-regions-v1', sizeMm=120, wallSizeMm=6, wedgeAngle=5,
        partitions=[{'from': [1144.439324, 197.608013], 'to': [1161.087386, 0]},
                    {'from': [1144.439324, 197.608013], 'to': [4644.439324, 700.222716]}], regions=[]),
        boundaries=[dict(name='edge_'+str(i+1), type=t) for i, t in enumerate(
            ['inlet']+['wall']*5+['reservoir', 'reservoir', 'outlet', 'axis'])])
    return sketch, cfg


def check_export(root, expected):
    (root/'system').mkdir(); (root/'constant').mkdir()
    (root/'system/controlDict').write_text(header('controlDict')+'application checkMesh; startFrom startTime; startTime 0; stopAt endTime; endTime 1; deltaT 1; writeControl timeStep; writeInterval 1;\n')
    (root/'system/fvSchemes').write_text(header('fvSchemes')+'ddtSchemes { default Euler; } gradSchemes { default Gauss linear; } divSchemes { default none; } laplacianSchemes { default Gauss linear corrected; } interpolationSchemes { default linear; } snGradSchemes { default corrected; }\n')
    (root/'system/fvSolution').write_text(header('fvSolution')+'solvers {}\n')
    result = subprocess.run(['gmshToFoam', str(root/'mesh.msh'), '-case', str(root)], capture_output=True, text=True)
    (root/'log.gmshToFoam').write_text(result.stdout+result.stderr)
    if result.returncode: raise AssertionError(result.stdout+result.stderr)
    boundary = root/'constant/polyMesh/boundary'; text = boundary.read_text()
    for name in ('wedgeFront', 'wedgeBack'):
        text, count = re.subn(r'(\b'+name+r'\s*\{[^{}]*?\btype\s+)\w+', r'\g<1>wedge', text, count=1)
        assert count == 1, name
    unmatched = re.search(r'defaultFaces\s*\{[^{}]*nFaces\s+(\d+)', text)
    assert not unmatched or int(unmatched[1]) == 0, 'Unassigned exterior faces'
    boundary.write_text(text)
    result = subprocess.run(['checkMesh', '-case', str(root), '-allTopology', '-allGeometry'], capture_output=True, text=True)
    (root/'log.checkMesh').write_text(result.stdout+result.stderr)
    assert result.returncode == 0 and 'Mesh OK' in result.stdout, result.stdout+result.stderr
    assert re.search(r'cells:\s+'+str(expected)+r'\b', result.stdout), result.stdout


class PartitionSweepRegression(unittest.TestCase):
    def test_oblique_shared_vertex_cuts_preview_edit_and_export(self):
        output = os.environ.get('VORTIA_REGION_TEST_OUTPUT')
        with tempfile.TemporaryDirectory(prefix='vortia-region-regression-') as temporary:
            root = Path(output or temporary).resolve(); root.mkdir(parents=True, exist_ok=True)
            for variant in ('original', 'reversed-small-angle'):
                with self.subTest(variant=variant):
                    sketch, cfg = shared_vertex_inputs()
                    if variant != 'original':
                        cfg['mesh']['wedgeAngle'] = .1
                        cfg['mesh']['partitions'] = [{'from': c['to'], 'to': c['from']} for c in reversed(cfg['mesh']['partitions'])]
                    before = copy.deepcopy(cfg)
                    preview = root/(variant+'-preview'); preview.mkdir()
                    result = mesh_case(preview, sketch, cfg, preview=True)
                    self.assertEqual(cfg, before)
                    self.assertEqual(len(result['regions']), 3)
                    self.assertEqual(result['internalInterfaces'], 2)
                    self.assertEqual(sum(r['cells'] for r in result['regions']), result['volumeCells'])
                    # Edit detected rows, as in the GUI: name, local size, method.
                    controls = copy.deepcopy(result['regions'])
                    for i, region in enumerate(controls):
                        region.update(name=['Nozzle', 'Jet', 'Far field'][i], sizeMm=[4, 60, 150][i], method='quads' if i < 2 else 'triangles')
                    cfg['mesh']['regions'] = controls
                    edited = root/(variant+'-edited'); edited.mkdir()
                    refined = mesh_case(edited, sketch, cfg, preview=True)
                    self.assertEqual([r['name'] for r in refined['regions']], [r['name'] for r in controls])
                    self.assertGreater(refined['regions'][0]['cells'], result['regions'][0]['cells'])
                    exported = root/(variant+'-final'); (exported/'geometry').mkdir(parents=True)
                    final = mesh_case(exported, sketch, cfg)
                    self.assertEqual(refined['nodes'], final['nodes'])
                    self.assertEqual(refined['cells'], final['cells'])
                    self.assertEqual(refined['volumeCells'], final['volumeCells'])
                    if os.environ.get('VORTIA_REGION_CHECK_OPENFOAM') == '1':
                        check_export(exported, final['volumeCells'])
                    print(variant, '3 editable regions,', final['volumeCells'], 'cells, preview/export identical', flush=True)


if __name__ == '__main__':
    unittest.main()
