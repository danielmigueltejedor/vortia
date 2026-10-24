import copy
import json
import math
from pathlib import Path
import tempfile
import unittest
from prepare import DEFAULT, geometry, wall, prepare


class NozzleReferenceTest(unittest.TestCase):
    def test_final_dimensions_and_tangencies(self):
        c=DEFAULT.copy(); g=geometry(c)
        self.assertAlmostEqual(wall(c,g,g['throatCenter'][0]),0.052,places=12)
        self.assertAlmostEqual(wall(c,g,c['nozzleExitX']),0.154,places=12)
        self.assertAlmostEqual(c['nozzleExitX']-g['throatEnd'][0],0.578,places=12)
        for name,slope in [('shoulderEnd',-math.tan(g['alpha'])),('throatStart',-math.tan(g['alpha'])),('throatEnd',math.tan(g['beta']))]:
            x=g[name][0]; eps=1e-8
            self.assertAlmostEqual((wall(c,g,x+eps)-wall(c,g,x-eps))/(2*eps),slope,places=5)

    def test_geometry_parameter_changes_really_move_the_mesh_wall(self):
        c=DEFAULT.copy(); c['throatHeight']=0.055; c['outletHeight']=0.160
        g=geometry(c)
        self.assertAlmostEqual(wall(c,g,g['throatCenter'][0]),0.055,places=12)
        self.assertAlmostEqual(wall(c,g,c['nozzleExitX']),0.160,places=12)

    def test_video_final_operating_pressure_and_temperature(self):
        self.assertEqual(DEFAULT['operatingPressure'],0)
        self.assertEqual(DEFAULT['inletTotalTemperature'],2800)
        self.assertEqual(DEFAULT['molecularWeight'],24)
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)/'case'; prepare(root,DEFAULT)
            p=(root/'0/p').read_text(); t=(root/'0/T').read_text()
            self.assertIn('p0 uniform 2580000.0',p)
            self.assertIn('p0 uniform 30000.0',p)
            self.assertIn('T0 uniform 2800.0',t)
            self.assertIn('SpalartAllmaras',(root/'constant/turbulenceProperties').read_text())
            self.assertIn('perfectGas',(root/'constant/thermophysicalProperties').read_text())
            self.assertIn('localEuler',(root/'system/fvSchemes').read_text())
            self.assertIn('machAwarePressureOutlet',p)
            self.assertIn('valueFraction()[i]',p)
            self.assertIn('vortiaCentralFoam',(root/'system/controlDict').read_text())
            self.assertTrue((root/'solver/vortiaCentralFoam.C').is_file())
            self.assertEqual((root/'.vortia-nozzle-case').read_text().strip(),'vortia-nozzle-video-2026-v2')
            s=json.loads((root/'geometry/working-sketch.vsketch.json').read_text())
            self.assertEqual(s['schemaVersion'],2)
            self.assertEqual(len(s['vertices']),len(s['segmentBulges']))
            self.assertTrue(s['closed'])
            self.assertAlmostEqual(s['vertices'][-1][0],4272.909)
            self.assertAlmostEqual(s['vertices'][-2][1],2154)

    def test_invalid_physics_and_geometry_create_nothing(self):
        for key,value in [('Cp',100),('mu',-1),('outletHeight',0),('throatRadius',5),('operatingPressure',-1),('inletHeight',float('nan')),('inletTotalTemperature',True)]:
            with self.subTest(key=key),tempfile.TemporaryDirectory() as d:
                c=copy.copy(DEFAULT);c[key]=value;root=Path(d)/'case'
                with self.assertRaises(ValueError):prepare(root,c)
                self.assertFalse(root.exists())

    def test_existing_files_are_never_overwritten(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);marker=root/'keep.txt';marker.write_text('local edits')
            with self.assertRaises(ValueError):prepare(root,DEFAULT)
            self.assertEqual(marker.read_text(),'local edits')
            self.assertEqual(list(root.iterdir()),[marker])


if __name__=='__main__':unittest.main()
