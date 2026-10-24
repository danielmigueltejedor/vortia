import copy
import unittest
from prepare import validate, fields
from pathlib import Path
import tempfile

def inputs():
    sketch = dict(units='mm',closed=True,vertices=[[0,0],[200,0],[200,50],[0,50]],segmentBulges=[0]*4)
    bc = lambda name,kind:dict(name=name,type=kind,pressure=101325,temperature=300,viscosityRatio=10)
    config = dict(material=dict(molecularWeight=28.966,Cp=1005,mu=1.8e-5,conductivity=.025,turbulence='SpalartAllmaras'),
        mesh=dict(sizeMm=10,wallSizeMm=2,wedgeAngle=5),
        solver=dict(iterations=1000,writeInterval=100,maxCo=.1),initial=dict(pressure=101325,temperature=300),
        boundaries=[bc('axis','axis'),bc('outlet','outlet'),bc('wall','wall'),bc('inlet','inlet')])
    return sketch,config

class GeneralFlowTest(unittest.TestCase):
    def test_general_dimensions_and_custom_material_are_accepted(self):
        sketch,cfg=inputs();cfg['material'].update(molecularWeight=24,Cp=1501)
        sketch['vertices'][1][0]=sketch['vertices'][2][0]=731
        self.assertEqual(set(validate(sketch,cfg)),{'axis','wall','inlet','outlet'})

    def test_invalid_inputs_cannot_reach_the_mesher(self):
        mutations=[
            lambda s,c:s.update(closed=False),
            lambda s,c:s['vertices'][0].__setitem__(1,-1),
            lambda s,c:c['boundaries'][0].update(type='wall'),
            lambda s,c:c['boundaries'][1].update(name='bad;name'),
            lambda s,c:c['boundaries'][1].update(pressure=float('nan')),
            lambda s,c:c['material'].update(Cp=1),
            lambda s,c:c['mesh'].update(sizeMm=.000001),
            lambda s,c:c['solver'].update(writeInterval=2000),
            lambda s,c:c['boundaries'][3].update(type='wall')]
        for change in mutations:
            s,c=inputs();change(s,c)
            with self.subTest(change=change),self.assertRaises(ValueError):validate(s,c)

    def test_region_names_cannot_merge_incompatible_conditions(self):
        s,c=inputs();c['boundaries'][3]['name']='outlet'
        with self.assertRaises(ValueError):validate(s,c)

    def test_generated_fields_use_each_boundary_setting(self):
        s,c=inputs();c['material'].update(molecularWeight=24,Cp=1501)
        c['boundaries'][3].update(pressure=2580000,temperature=2800)
        c['boundaries'][1].update(pressure=30000,temperature=300)
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/'0').mkdir();(root/'constant').mkdir()
            fields(root,c,validate(s,c))
            self.assertIn('p0 uniform 2580000',(root/'0/p').read_text())
            self.assertIn('T0 uniform 2800',(root/'0/T').read_text())
            self.assertIn('machAwarePressureOutlet',(root/'0/p').read_text())
            self.assertIn('molWeight 24',(root/'constant/thermophysicalProperties').read_text())
            self.assertNotIn('axis {',(root/'0/U').read_text())

    def test_total_pressure_inlet_uses_a_bounded_normal_characteristic_velocity(self):
        s,c=inputs();c['boundaries'][3].update(pressure=3640000,temperature=2900)
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/'0').mkdir();(root/'constant').mkdir()
            fields(root,c,validate(s,c))
            velocity=(root/'0/U').read_text()
            self.assertIn('name reservoirCharacteristicVelocity_inlet;',velocity)
            self.assertIn('totalT = 2900;',velocity)
            self.assertIn('const scalar q = min(cstar',velocity)
            self.assertIn('ub[i] = -n[i]*q;',velocity)
            self.assertIn('p0 uniform 3640000',(root/'0/p').read_text())
            self.assertIn('T0 uniform 2900',(root/'0/T').read_text())

if __name__=='__main__':unittest.main()
