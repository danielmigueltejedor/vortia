#!/usr/bin/env python3
"""Create a new axisymmetric ideal-gas case from an arbitrary closed CAD sketch.

Gmsh runs in this separate process. No tutorial dimensions or material values
are embedded. Existing destinations are never replaced. CFD remains experimental.
"""
import argparse
import json
import math
from pathlib import Path
import re
import shutil
import subprocess
import sys

from boundaries import header, bisect, reservoir_velocity, pressure_outlet, viscosity_ratio_inlet


def finite(value, positive=True):
    if type(value) not in (float, int) or not math.isfinite(value) or (positive and value <= 0):
        raise ValueError('A numerical setting is missing, non-finite or outside its valid range')
    return value


def validate(sketch, cfg):
    pts = sketch['vertices']
    bulges = sketch.get('segmentBulges', [0]*len(pts))
    if not sketch.get('closed') or sketch.get('units') != 'mm' or not 3 <= len(pts) <= 512:
        raise ValueError('Use one closed fluid contour in millimetres, with 3 to 512 vertices')
    if len(bulges) != len(pts) or len(cfg['boundaries']) != len(pts):
        raise ValueError('Assign a boundary condition to every sketch segment')
    for pt in pts:
        if len(pt) != 2 or any(type(v) not in (int, float) or not math.isfinite(v) for v in pt) or pt[1] < 0:
            raise ValueError('An axisymmetric fluid contour must lie at radius y >= 0')
    extent_x=max(p[0] for p in pts)-min(p[0] for p in pts)
    extent_y=max(p[1] for p in pts)-min(p[1] for p in pts)
    if min(extent_x,extent_y)<=1e-6: raise ValueError('The fluid domain has zero extent')
    groups = {}
    for i, (bulge, bc) in enumerate(zip(bulges, cfg['boundaries'])):
        finite(bulge, False)
        if abs(bulge) > 1 or math.dist(pts[i], pts[(i+1) % len(pts)]) < 1e-6:
            raise ValueError('Invalid arc or coincident adjacent vertices')
        name, kind = bc['name'], bc['type']
        if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{0,47}', name) or name in ('wedgeFront', 'wedgeBack', 'fluid', 'defaultFaces'):
            raise ValueError('Boundary names must be OpenFOAM words without spaces')
        if kind not in ('wall', 'axis', 'inlet', 'outlet', 'reservoir'):
            raise ValueError('Unsupported boundary condition')
        on_axis = pts[i][1] == pts[(i+1) % len(pts)][1] == 0 and bulge == 0
        if (kind == 'axis') != on_axis:
            raise ValueError('Segments on y=0 must be Axis; Axis segments must be straight and on y=0')
        for key in ('pressure', 'temperature', 'viscosityRatio'):
            finite(bc[key])
        if bc['viscosityRatio']>100000: raise ValueError('The turbulent viscosity ratio is excessive')
        if name in groups and groups[name] != bc:
            raise ValueError('Segments with the same region name must have identical boundary settings')
        groups[name] = bc
    if not any(b['type'] == 'inlet' for b in groups.values()) or not any(b['type'] == 'outlet' for b in groups.values()):
        raise ValueError('Assign at least one total-pressure inlet and one pressure outlet')
    mat, mesh, solver = cfg['material'], cfg['mesh'], cfg['solver']
    for key in ('molecularWeight', 'Cp', 'mu', 'conductivity'):
        finite(mat[key])
    if mat['Cp'] <= 8314.462618/mat['molecularWeight']:
        raise ValueError('Cp must exceed the specific gas constant')
    for key in ('sizeMm', 'wallSizeMm', 'wedgeAngle'):
        finite(mesh[key])
    if max(extent_x,extent_y)/mesh['sizeMm']>2000 or sum(math.dist(pts[i],pts[(i+1)%len(pts)]) for i in range(len(pts)))/mesh['wallSizeMm']>100000:
        raise ValueError('The requested mesh resolution is excessive; increase the cell sizes')
    if not 0.1 <= mesh['wedgeAngle'] <= 5 or mesh['wallSizeMm'] > mesh['sizeMm']:
        raise ValueError('Use a wedge angle between 0.1 and 5 degrees and a wall size no larger than the global size')
    if not 1 <= finite(solver['iterations']) <= 200000 or not 1 <= finite(solver['writeInterval']) <= solver['iterations']:
        raise ValueError('Invalid iteration limit or output interval')
    if not 0 < finite(solver['maxCo']) <= 0.5:
        raise ValueError('Use an acoustic Courant number in (0, 0.5]')
    if mat['turbulence'] not in ('laminar', 'SpalartAllmaras'):
        raise ValueError('Unsupported turbulence model')
    finite(cfg['initial']['pressure']); finite(cfg['initial']['temperature'])
    return groups


def mesh_case(root, sketch, cfg):
    import gmsh
    gmsh.initialize(['vortia-mesher', '-nopopup'])
    try:
        gmsh.model.add('fluid_domain')
        geo = gmsh.model.geo
        pts = [[x/1000, y/1000] for x, y in sketch['vertices']]
        tags = [geo.addPoint(x, y, 0) for x, y in pts]
        curves = []
        for i, a in enumerate(pts):
            j = (i+1) % len(pts); b = pts[j]
            bulge = sketch.get('segmentBulges', [0]*len(pts))[i]
            if abs(bulge) < 1e-14:
                curves.append(geo.addLine(tags[i], tags[j]))
            else:
                k = (1-bulge*bulge)/(4*bulge)
                cx = (a[0]+b[0])/2 - (b[1]-a[1])*k
                cy = (a[1]+b[1])/2 + (b[0]-a[0])*k
                center = geo.addPoint(cx, cy, 0)
                curves.append(geo.addCircleArc(tags[i], center, tags[j], nx=0, ny=0, nz=1 if bulge > 0 else -1))
        surface = geo.addPlaneSurface([geo.addCurveLoop(curves)])
        angle = math.radians(cfg['mesh']['wedgeAngle'])
        geo.rotate([(2, surface)], 0, 0, 0, 1, 0, 0, -angle/2)
        extruded = geo.revolve([(2, surface)], 0, 0, 0, 1, 0, 0, angle, [1], [], True)
        geo.synchronize()
        top = extruded[0][1]
        volume = next(tag for dim, tag in extruded if dim == 3)
        physical = {'wedgeFront': [surface], 'wedgeBack': [top]}
        for dim, tag in extruded:
            if dim != 2 or tag == top: continue
            edges = {abs(e) for _, e in gmsh.model.getBoundary([(2, tag)], oriented=False)}
            matches = [i for i, curve in enumerate(curves) if curve in edges]
            if len(matches) != 1:
                raise ValueError('Cannot associate an extruded face with its CAD boundary')
            bc = cfg['boundaries'][matches[0]]
            physical.setdefault(bc['name'], []).append(tag)
        for name, faces in physical.items():
            gmsh.model.addPhysicalGroup(2, faces, name=name)
        gmsh.model.addPhysicalGroup(3, [volume], name='fluid')
        size = cfg['mesh']['sizeMm']/1000
        gmsh.option.setNumber('Mesh.MeshSizeMax', size)
        gmsh.option.setNumber('Mesh.MeshSizeMin', cfg['mesh']['wallSizeMm']/1000)
        gmsh.option.setNumber('Mesh.MeshSizeFromPoints', 0)
        gmsh.option.setNumber('Mesh.MeshSizeFromCurvature', 0)
        walls = [curve for curve, bc in zip(curves, cfg['boundaries']) if bc['type'] == 'wall']
        if walls:
            distance = gmsh.model.mesh.field.add('Distance')
            gmsh.model.mesh.field.setNumbers(distance, 'CurvesList', walls)
            gmsh.model.mesh.field.setNumber(distance, 'Sampling', 100)
            threshold = gmsh.model.mesh.field.add('Threshold')
            for key, value in {'InField': distance, 'SizeMin': cfg['mesh']['wallSizeMm']/1000,
                               'SizeMax': size, 'DistMin': size, 'DistMax': 10*size}.items():
                gmsh.model.mesh.field.setNumber(threshold, key, value)
            gmsh.model.mesh.field.setAsBackgroundMesh(threshold)
        gmsh.option.setNumber('Mesh.MshFileVersion', 2.2)
        gmsh.model.mesh.generate(3)
        gmsh.write(str(root/'mesh.msh'))
        gmsh.write(str(root/'geometry/domain.geo_unrolled'))
        count = sum(len(tags) for tags in gmsh.model.mesh.getElements(3)[1])
        if count > 1000000:
            raise ValueError('The generated mesh exceeds 1,000,000 cells; increase the mesh size')
        (root/'mesh-report.json').write_text(json.dumps({'mesher': 'Gmsh', 'version': gmsh.__version__,
            'cells': count, 'wedgeAngle': cfg['mesh']['wedgeAngle'], 'scope': 'mesh generation only; quality checked by OpenFOAM'}, indent=2)+'\n')
    finally:
        gmsh.finalize()


def fields(root, cfg, groups):
    mat = cfg['material']; gasr = 8314.462618/mat['molecularWeight']
    gamma = mat['Cp']/(mat['Cp']-gasr)
    def field(name, dimensions, initial, bcs, vector=False):
        text = header(name, 'volVectorField' if vector else 'volScalarField')
        text += f'dimensions {dimensions};\ninternalField uniform {initial};\nboundaryField\n{{\n'
        for patch, bc in bcs.items(): text += f'{patch} {{ {bc} }}\n'
        text += 'wedgeFront { type wedge; }\nwedgeBack { type wedge; }\n}\n'
        (root/'0'/name).write_text(text)
    bcs = {name: {} for name in ('p', 'U', 'T', 'nuTilda', 'nut', 'alphat')}
    for name, bc in groups.items():
        kind = bc['type']
        if kind == 'axis': continue
        p, t = bc['pressure'], bc['temperature']
        local = dict(mat, exteriorGaugePressure=p, operatingPressure=0,
                     exteriorTotalTemperature=t, turbulentViscosityRatio=bc['viscosityRatio'])
        chi = bisect(lambda x: x**4/(x**3+7.1**3)-bc['viscosityRatio'], 1e-8, 1e6)
        nu = chi*mat['mu']*gasr*t/p
        bcs['p'][name] = ('type zeroGradient;' if kind == 'wall' else pressure_outlet(local) if kind == 'outlet'
            else f'type totalPressure; p0 uniform {p}; U U; phi phi; rho rho; psi thermo:psi; gamma {gamma}; value uniform {p};')
        bcs['U'][name] = ('type noSlip;' if kind == 'wall' else reservoir_velocity(local) if kind in ('reservoir','inlet')
            else 'type pressureInletOutletVelocity; value uniform (0 0 0);')
        bcs['T'][name] = ('type zeroGradient;' if kind == 'wall' else
            f'type {"totalTemperature" if kind == "inlet" else "inletOutletTotalTemperature"}; T0 uniform {t}; U U; phi phi; psi thermo:psi; gamma {gamma}; value uniform {t};')
        bcs['nuTilda'][name] = 'type fixedValue; value uniform 0;' if kind == 'wall' else viscosity_ratio_inlet(local, chi, nu)
        for field_name in ('p','U','nuTilda'):
            for code_name in ('machAwarePressureOutlet','reservoirCharacteristicVelocity','viscosityRatioNuTilda'):
                bcs[field_name][name]=bcs[field_name][name].replace('name '+code_name+';', 'name '+code_name+'_'+name+';')
        bcs['nut'][name] = ('type nutUSpaldingWallFunction;' if kind == 'wall' and mat['turbulence'] != 'laminar' else 'type calculated;')+' value uniform 0;'
        bcs['alphat'][name] = ('type compressible::alphatWallFunction; Prt 0.85;' if kind == 'wall' and mat['turbulence'] != 'laminar' else 'type calculated;')+' value uniform 0;'
    initial = cfg['initial']; rho = initial['pressure']/(gasr*initial['temperature'])
    for name, dims, value in [('p', '[1 -1 -2 0 0 0 0]', initial['pressure']),
                              ('T', '[0 0 0 1 0 0 0]', initial['temperature']),
                              ('nuTilda', '[0 2 -1 0 0 0 0]', 10*mat['mu']/rho),
                              ('nut', '[0 2 -1 0 0 0 0]', 0), ('alphat', '[1 -1 -1 0 0 0 0]', 0)]:
        field(name, dims, value, bcs[name])
    field('U', '[0 1 -1 0 0 0 0]', '(0 0 0)', bcs['U'], True)
    (root/'constant/thermophysicalProperties').write_text(header('thermophysicalProperties')+f'''
thermoType {{ type hePsiThermo; mixture pureMixture; transport const; thermo hConst;
 equationOfState perfectGas; specie specie; energy sensibleInternalEnergy; }}
mixture {{ specie {{ molWeight {mat['molecularWeight']}; }} thermodynamics {{ Cp {mat['Cp']}; Hf 0; }}
 transport {{ mu {mat['mu']}; Pr {mat['mu']*mat['Cp']/mat['conductivity']}; }} }}
''')
    turbulence = 'simulationType laminar;\n' if mat['turbulence'] == 'laminar' else 'simulationType RAS;\nRAS { RASModel SpalartAllmaras; turbulence on; printCoeffs on; }\n'
    (root/'constant/turbulenceProperties').write_text(header('turbulenceProperties')+turbulence)


def prepare(root, sketch, cfg, sources):
    groups = validate(sketch, cfg)
    root = Path(root)
    if root.exists() and any(root.iterdir()): raise ValueError('Refusing to replace an existing case')
    if any(c.isspace() for c in str(root.resolve())): raise ValueError('Use a project folder without spaces for OpenFOAM build tools')
    for name in ('0', 'constant', 'system', 'geometry'): (root/name).mkdir(parents=True, exist_ok=True)
    (root/'geometry/working-sketch.vsketch.json').write_text(json.dumps(sketch, indent=2)+'\n')
    (root/'flow-setup.json').write_text(json.dumps(cfg, indent=2)+'\n')
    shutil.copytree(sources, root/'solver')
    (root/'system/controlDict').write_text(header('controlDict')+'application vortiaCentralFoam; startFrom startTime; startTime 0; stopAt endTime; endTime 1; deltaT 1; writeControl timeStep; writeInterval 1;\n')
    mesh_case(root, sketch, cfg)
    with (root/'log.gmshToFoam').open('w') as log:
        subprocess.run(['gmshToFoam', str((root/'mesh.msh').resolve()), '-case', str(root.resolve())], stdout=log, stderr=subprocess.STDOUT, check=True)
    boundary = root/'constant/polyMesh/boundary'
    text = boundary.read_text()
    for name, kind in [('wedgeFront', 'wedge'), ('wedgeBack', 'wedge'), *[(n, 'wall') for n, b in groups.items() if b['type'] == 'wall']]:
        text, count = re.subn(r'(\b'+re.escape(name)+r'\s*\{[^{}]*?\btype\s+)\w+', r'\g<1>'+kind, text, count=1)
        if count != 1: raise ValueError('Missing mesh boundary '+name)
    unexpected = re.search(r'defaultFaces\s*\{[^{}]*nFaces\s+(\d+)', text)
    if unexpected and int(unexpected[1]): raise ValueError('Unassigned faces were produced by mesh conversion')
    boundary.write_text(text)
    fields(root, cfg, groups)
    s = cfg['solver']
    (root/'system/fvSchemes').write_text(header('fvSchemes')+'''
fluxScheme Kurganov;
ddtSchemes { default localEuler; }
gradSchemes { default cellLimited Gauss linear 1; }
divSchemes { default none; div(tauMC) Gauss linear; div(phi,nuTilda) bounded Gauss upwind;
 div(((rho*nuEff)*dev2(T(grad(U))))) Gauss linear; }
laplacianSchemes { default Gauss linear limited 0.5; }
interpolationSchemes { default linear; reconstruct(rho) Minmod; reconstruct(U) Minmod; reconstruct(T) Minmod; }
snGradSchemes { default limited 0.5; } wallDist { method meshWave; }
''')
    (root/'system/fvSolution').write_text(header('fvSolution')+'''
solvers { "(rho|rhoU|rhoE)" { solver diagonal; }
 "(U|e|nuTilda)" { solver smoothSolver; smoother symGaussSeidel; tolerance 1e-9; relTol 0; } }
''')
    functions = 'extrema { type fieldMinMax; libs (fieldFunctionObjects); fields (p T U); mode magnitude; writeControl timeStep; writeInterval 100; }\n'
    functions += 'Mach { type MachNo; libs (fieldFunctionObjects); result Mach; executeControl writeTime; writeControl writeTime; }\n'
    functions += 'wallResolution { type yPlus; libs (fieldFunctionObjects); executeControl writeTime; writeControl writeTime; }\n'
    for name, bc in groups.items():
        if bc['type'] not in ('wall', 'axis'):
            functions += f'mass_{name} {{ type surfaceFieldValue; libs (fieldFunctionObjects); regionType patch; name {name}; operation sum; fields (phi); writeFields false; writeControl timeStep; writeInterval 100; }}\n'
    (root/'system/controlDict').write_text(header('controlDict')+f'''
application vortiaCentralFoam; noSwirl true;
startFrom latestTime; startTime 0; stopAt endTime; endTime {s['iterations']}; deltaT 1;
adjustTimeStep no; maxCo {s['maxCo']}; maxDeltaT 1e-6; rDeltaTSmoothingCoeff 0.5; maxDiffusionCo 0.1;
writeControl timeStep; writeInterval {s['writeInterval']}; purgeWrite 0;
writeFormat ascii; writePrecision 9; writeCompression off; timeFormat general; timePrecision 9; runTimeModifiable true;
functions {{ {functions} }}
''')
    (root/'.vortia-flow-case').write_text('vortia-axisymmetric-flow-v1\n')
    (root/'README.md').write_text('# General axisymmetric flow case\n\nGenerated from the saved sketch and flow-setup.json. Ideal gas, energy, adiabatic no-slip walls and optional Spalart–Allmaras. Pressures are absolute Pa. The initial field is a uniform starting guess, not a simulation result. The custom OpenFOAM v2606 helper enforces no swirl and includes viscous energy transport. CFD convergence, mass balance and mesh dependence must be checked. The numerical backend is experimental; no Fluent equivalence is asserted.\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sketch', type=Path, required=True); parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--case', type=Path, required=True); parser.add_argument('--solver-sources', type=Path, required=True)
    parser.add_argument('--gmsh-runtime', type=Path)
    args = parser.parse_args()
    if args.gmsh_runtime: sys.path.insert(0, str(args.gmsh_runtime.resolve()))
    try:
        prepare(args.case, json.loads(args.sketch.read_text()), json.loads(args.config.read_text()), args.solver_sources)
    except Exception as error:
        print('ERROR: '+str(error),file=sys.stderr)
        sys.exit(1)
