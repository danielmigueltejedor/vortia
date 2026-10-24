#!/usr/bin/env python3
"""Reconstruct the supplied tutorial's dimensions and prepare an OpenFOAM wedge.

No solver results are synthesized. Run blockMesh/checkMesh/rhoCentralFoam afterwards.
The radii read during the tutorial's fillet operation are retained when solving the
final dimensions. The absent original CAD file prevents bitwise CAD equivalence.
"""
import argparse
import json
import math
import shutil
from pathlib import Path


def bisect(f, lo, hi):
    if f(lo) * f(hi) > 0:
        raise ValueError("The dimensions do not admit the requested tangent geometry")
    for _ in range(80):
        mid = (lo + hi) / 2
        if f(lo) * f(mid) <= 0:
            hi = mid
        else:
            lo = mid
    return (lo + hi) / 2


DEFAULT = json.loads(Path(__file__).with_name('reference.json').read_text())['values']
SOLVER_FILES = ['vortiaCentralFoam.C','centralCourantNo.H','createFieldRefs.H',
                'createFields.H','directionInterpolate.H','readFluxScheme.H',
                'setRDeltaT.H','fixedRhoFvPatchScalarField.H','Make/files','Make/options',
                'README.md','LICENSE.md']


def validate(c):
    if set(c) != set(DEFAULT) or any(type(v) not in (int,float) or not math.isfinite(v) for v in c.values()):
        raise ValueError('Unknown, missing or non-finite case parameters')
    if any(v <= 0 for k,v in c.items() if 'Gauge' not in k and k != 'operatingPressure'):
        raise ValueError('Geometry and material parameters must be positive')
    gas_r = 8314.462618/c['molecularWeight']
    if c['Cp'] <= gas_r or c['operatingPressure'] < 0:
        raise ValueError('Cp must exceed the specific gas constant and operating pressure cannot be negative')
    p0=c['operatingPressure']+c['inletGaugeTotalPressure']
    pa=c['operatingPressure']+c['exteriorGaugePressure']
    if not p0 > pa > 0:
        raise ValueError('The absolute inlet total pressure must exceed the positive exterior pressure')


def geometry(c):
    validate(c)
    r1, r2 = c['shoulderRadius'], c['throatRadius']
    x_b = c['nozzleExitX'] - c['straightDivergingLength']
    cy = c['throatHeight'] + r2
    beta = bisect(lambda b: math.tan(b)*c['straightDivergingLength']
                  - (c['outletHeight'] - cy + r2*math.cos(b)), 0, 0.6)
    cx = x_b - r2*math.sin(beta)
    c1 = (c['inletPlateau'], c['inletHeight']-r1)
    alpha = bisect(lambda a: (cx-c1[0])*math.sin(a)+(cy-c1[1])*math.cos(a)-r1-r2,
                   0, math.atan2(cx-c1[0], cy-c1[1]))
    a = (c1[0]+r1*math.sin(alpha), c1[1]+r1*math.cos(alpha))
    b = (cx-r2*math.sin(alpha), cy-r2*math.cos(alpha))
    d = (cx+r2*math.sin(beta), cy-r2*math.cos(beta))
    if not 0 < c1[0] < a[0] < b[0] < cx < d[0] < c['nozzleExitX']:
        raise ValueError('Overlapping fillets or reversed wall sections')
    return dict(alpha=alpha, beta=beta, shoulderCenter=c1,
                throatCenter=(cx,cy), shoulderEnd=a, throatStart=b, throatEnd=d)


def wall(c, g, x):
    r1, r2 = c['shoulderRadius'], c['throatRadius']
    if x <= c['inletPlateau']: return c['inletHeight']
    if x <= g['shoulderEnd'][0]:
        return g['shoulderCenter'][1]+math.sqrt(max(0,r1*r1-(x-g['shoulderCenter'][0])**2))
    if x <= g['throatStart'][0]:
        return g['shoulderEnd'][1]-(x-g['shoulderEnd'][0])*math.tan(g['alpha'])
    if x <= g['throatEnd'][0]:
        return g['throatCenter'][1]-math.sqrt(max(0,r2*r2-(x-g['throatCenter'][0])**2))
    return g['throatEnd'][1]+(x-g['throatEnd'][0])*math.tan(g['beta'])


def header(name, cls='dictionary'):
    return f'FoamFile {{ version 2.0; format ascii; class {cls}; object {name}; }}\n'


def reservoir_velocity(c):
    """Subsonic normal reservoir inflow using the outgoing Riemann invariant.

    Extrapolating the hot jet velocity into the 300 K reservoir can demand more
    kinetic energy than its total enthalpy. Compute the inflow speed from the
    interior outgoing characteristic and T0 instead. At choking use M=1;
    outflow extrapolates velocity. Native totalPressure and mixed total
    temperature provide the associated thermodynamic boundary values.
    See NASA AIAA 2018-0334, stagnation-properties subsonic inflow.
    This small generated OpenFOAM boundary requires its wmake toolchain.
    """
    gas_r=8314.462618/c['molecularWeight']
    gamma=c['Cp']/(c['Cp']-gas_r)
    return '''type codedFixedValue;
value uniform (0 0 0);
name reservoirCharacteristicVelocity;
code
#{
 const scalar gamma = @GAMMA@, gasR = @R@, totalT = @T0@;
 const auto& T = patch().lookupPatchField<volScalarField>("T");
 const scalarField ti(T.patchInternalField());
 const vectorField ui(this->patchInternalField());
 const vectorField n(patch().nf());
 vectorField ub(ui);
 const scalar cstar = sqrt(2*gamma*gasR*totalT/(gamma+1));
 forAll(ub,i) {
  const scalar un = ui[i]&n[i];
  if (un <= 0) {
   const scalar ri = un+2*sqrt(gamma*gasR*max(ti[i],scalar(1)))/(gamma-1);
   const scalar disc = -2*(gamma-1)*sqr(ri)+4*(gamma+1)*gamma*gasR*totalT/(gamma-1);
   const scalar q = min(cstar,max(scalar(0),(-(gamma-1)*ri+sqrt(max(disc,scalar(0))))/(gamma+1)));
   ub[i] = -n[i]*q;
  }
 }
 operator==(ub);
#};'''.replace('@GAMMA@',repr(gamma)).replace('@R@',repr(gas_r)).replace('@T0@',repr(c['exteriorTotalTemperature']))


def pressure_outlet(c):
    # Fluent's pressure outlet extrapolates static pressure at supersonic
    # outflow. A fixedValue on the entire jet exit over-specifies that flow.
    gas_r=8314.462618/c['molecularWeight']
    gamma=c['Cp']/(c['Cp']-gas_r)
    pfar=c['operatingPressure']+c['exteriorGaugePressure']
    return '''type codedMixed;
name machAwarePressureOutlet;
refValue uniform @P@; refGradient uniform 0; valueFraction uniform 1; value uniform @P@;
code
#{
 const auto& u = patch().lookupPatchField<volVectorField>("U");
 const auto& t = patch().lookupPatchField<volScalarField>("T");
 const vectorField n(patch().nf());
 this->refValue() = @P@;
 this->refGrad() = 0;
 forAll(this->valueFraction(),i)
 {
  const scalar c = sqrt(@GAMMA@*@R@*max(t[i],scalar(1)));
  this->valueFraction()[i] = ((u[i]&n[i]) >= c) ? 0 : 1;
 }
#};'''.replace('@P@',repr(pfar)).replace('@GAMMA@',repr(gamma)).replace('@R@',repr(gas_r))


def viscosity_ratio_inlet(c, chi, initial):
    return '''type codedMixed;
name viscosityRatioNuTilda;
refValue uniform @INITIAL@; refGradient uniform 0; valueFraction uniform 1; value uniform @INITIAL@;
code
#{
 const auto& u = patch().lookupPatchField<volVectorField>("U");
 const auto& rho = patch().lookupPatchField<volScalarField>("rho");
 const vectorField n(patch().nf());
 this->refGrad() = 0;
 forAll(this->valueFraction(),i)
 {
  this->refValue()[i] = @CHI_MU@/max(rho[i],scalar(1e-12));
  this->valueFraction()[i] = ((u[i]&n[i]) <= 0) ? 1 : 0;
 }
#};'''.replace('@INITIAL@',repr(initial)).replace('@CHI_MU@',repr(chi*c['mu']))


def prepare(root, c, refinement=1, end_time=50000):
    if refinement not in (1,2,3) or not math.isfinite(end_time) or not 1 <= end_time <= 200000:
        raise ValueError("Invalid refinement or iteration limit")
    g = geometry(c)
    root = Path(root)
    if any(ch.isspace() for ch in str(root.resolve())):
        raise ValueError('Choose a case path without whitespace for the OpenFOAM build tools')
    if root.exists() and any(root.iterdir()):
        raise ValueError('Refusing to replace an existing case; choose a new empty directory')
    source_dir=Path(__file__).with_name('solver')
    if any(not (source_dir/name).is_file() for name in SOLVER_FILES):
        raise ValueError('The OpenFOAM v2606 nozzle solver sources are missing')
    for folder in ['0','constant','system','geometry']:
        (root/folder).mkdir(parents=True, exist_ok=True)
    def put(path, text): (root/path).write_text(text)
    for name in SOLVER_FILES:
        target=root/'solver'/name
        target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(source_dir/name,target)
    put('parameters.json', json.dumps(c, indent=2)+'\n')
    put('geometry/reconstruction.json', json.dumps(g, indent=2)+'\n')
    xe, xmax = c['nozzleExitX'], c['nozzleExitX']+c['exteriorLength']
    ymax = c['outletHeight']+c['exteriorHeightAboveExit']
    xy = [(0,0),(0,c['inletHeight']),(c['inletPlateau'],c['inletHeight']),
          g['shoulderEnd'],g['throatStart'],g['throatEnd'],(xe,c['outletHeight']),
          (xe,ymax),(xmax,ymax),(xmax,0)]
    bulges = [0,0,math.tan(-g['alpha']/4),0,math.tan((g['alpha']+g['beta'])/4),0,0,0,0,0]
    sketch = dict(schemaVersion=2, units='mm', closed=True,
                  vertices=[[x*1000,y*1000] for x,y in xy],segmentBulges=bulges)
    put('geometry/working-sketch.vsketch.json',json.dumps(sketch,indent=2)+'\n')

    # Conformal structured wedge: resolved nozzle, downstream jet band, outer domain.
    # Axial intervals end at each tangent and at the true minimum of the throat arc.
    xs = [0,c['inletPlateau'],g['shoulderEnd'][0],g['throatStart'][0],
          g['throatCenter'][0],g['throatEnd'][0],xe]
    theta = math.radians(2.5)
    verts, lookup, blocks, arcs = [], {}, [], []
    patches = {k:[] for k in ['inlet','wall','farfield','outlet','axis','wedgeFront','wedgeBack']}
    def vertex(x,r,side):
        p = (x,r*math.cos(theta),side*r*math.sin(theta))
        key = tuple(round(v,13) for v in p)
        if key not in lookup:
            lookup[key] = len(verts); verts.append(p)
        return lookup[key]
    def block(x0,x1,r00,r10,r01,r11,nx,ny,grading=1,toparc=None):
        v = [vertex(x0,r00,-1),vertex(x1,r10,-1),vertex(x1,r11,-1),vertex(x0,r01,-1),
             vertex(x0,r00,1),vertex(x1,r10,1),vertex(x1,r11,1),vertex(x0,r01,1)]
        blocks.append(f"hex ({' '.join(map(str,v))}) ({nx} {ny} 1) simpleGrading (1 {grading} 1)")
        patches['wedgeFront'].append([v[i] for i in [0,3,2,1]])
        patches['wedgeBack'].append([v[i] for i in [4,5,6,7]])
        if r00 == r10 == 0: patches['axis'].append([v[i] for i in [0,1,5,4]])
        if toparc:
            for side,ia,ib in [(-1,3,2),(1,7,6)]:
                xm=(x0+x1)/2; rm=wall(c,g,xm)
                pm=(xm,rm*math.cos(theta),side*rm*math.sin(theta))
                arcs.append(f"arc {v[ia]} {v[ib]} ({' '.join(f'{a:.13g}' for a in pm)})")
        return v
    nr = 20*refinement
    for i,(x0,x1) in enumerate(zip(xs,xs[1:])):
        nx=max(3,math.ceil((x1-x0)/0.008))*refinement
        is_arc = i in (1,3,4)
        v=block(x0,x1,0,0,wall(c,g,x0),wall(c,g,x1),nx,nr,0.003,toparc=is_arc)
        patches['wall'].append([v[k] for k in [3,7,6,2]])
        if i==0: patches['inlet'].append([v[k] for k in [0,4,7,3]])
    # The internal interface is a straight continuation of the nozzle exit height.
    njet=110*refinement
    v=block(xe,xmax,0,0,c['outletHeight'],c['outletHeight'],njet,nr,0.003)
    patches['outlet'].append([v[k] for k in [1,2,6,5]])
    v=block(xe,xmax,c['outletHeight'],c['outletHeight'],ymax,ymax,njet,40*refinement,1500)
    patches['farfield'] += [[v[k] for k in [0,4,7,3]],[v[k] for k in [3,7,6,2]]]
    patches['outlet'].append([v[k] for k in [1,2,6,5]])
    text=header('blockMeshDict')+'mergeType points;\nscale 1;\nvertices\n(\n'
    text+='\n'.join('('+ ' '.join(f'{a:.13g}' for a in p)+')' for p in verts)+'\n);\nblocks\n(\n'
    text+='\n'.join(blocks)+'\n);\nedges\n(\n'+'\n'.join(arcs)+'\n);\nboundary\n(\n'
    for name,faces in patches.items():
        typ='wedge' if name.startswith('wedge') else ('wall' if name=='wall' else ('empty' if name=='axis' else 'patch'))
        text+=f'{name}\n{{ type {typ}; faces (\n'+'\n'.join('('+' '.join(map(str,f))+')' for f in faces)+'\n); }\n'
    text+='\n);\nmergePatchPairs ();\n'
    put('system/blockMeshDict',text)
    gamma=c['Cp']/(c['Cp']-8314.462618/c['molecularWeight'])
    pfar=c['operatingPressure']+c['exteriorGaugePressure']
    p0=c['operatingPressure']+c['inletGaugeTotalPressure']
    def field(name,dim,initial,bcs,vector=False):
        cls='volVectorField' if vector else 'volScalarField'
        txt=header(name,cls)+f'dimensions {dim};\ninternalField uniform {initial};\nboundaryField\n{{\n'
        for patch in patches:
            bc = 'type wedge;' if patch.startswith('wedge') else ('type empty;' if patch=='axis' else bcs[patch])
            txt+=f'{patch} {{ {bc} }}\n'
        put('0/'+name,txt+'}\n')
    tp=lambda pressure:f'type totalPressure; p0 uniform {pressure}; U U; phi phi; rho rho; psi thermo:psi; gamma {gamma}; value uniform {pressure};'
    tt=lambda temperature:f'type totalTemperature; T0 uniform {temperature}; U U; phi phi; psi thermo:psi; gamma {gamma}; value uniform {temperature};'
    field('p','[1 -1 -2 0 0 0 0]',pfar,dict(inlet=tp(p0),farfield=tp(pfar),wall='type zeroGradient;',
          outlet=pressure_outlet(c)))
    vel='type pressureInletOutletVelocity; value uniform (0 0 0);'
    field('U','[0 1 -1 0 0 0 0]','(0 0 0)',dict(inlet=vel,farfield=reservoir_velocity(c),outlet=vel,wall='type noSlip;'),True)
    ambient_t=c['exteriorTotalTemperature']
    field('T','[0 0 0 1 0 0 0]',ambient_t,dict(inlet=tt(c['inletTotalTemperature']),farfield=tt(ambient_t).replace('type totalTemperature;','type inletOutletTotalTemperature;'),
          outlet=tt(ambient_t).replace('type totalTemperature;','type inletOutletTotalTemperature;'),wall='type zeroGradient;'))
    chi=bisect(lambda x:x**4/(x**3+7.1**3)-c['turbulentViscosityRatio'],1e-4,1000)
    nu=lambda p,t:chi*c['mu']*8314.462618/c['molecularWeight']*t/p
    field('nuTilda','[0 2 -1 0 0 0 0]',nu(pfar,ambient_t),dict(inlet=viscosity_ratio_inlet(c,chi,nu(p0,c['inletTotalTemperature'])),
          farfield=viscosity_ratio_inlet(c,chi,nu(pfar,ambient_t)),
          outlet=viscosity_ratio_inlet(c,chi,nu(pfar,ambient_t)),wall='type fixedValue; value uniform 0;'))
    field('nut','[0 2 -1 0 0 0 0]',0,dict(inlet='type calculated; value uniform 0;',farfield='type calculated; value uniform 0;',
          outlet='type calculated; value uniform 0;',wall='type nutUSpaldingWallFunction; value uniform 0;'))
    field('alphat','[1 -1 -1 0 0 0 0]',0,dict(inlet='type calculated; value uniform 0;',farfield='type calculated; value uniform 0;',
          outlet='type calculated; value uniform 0;',wall='type compressible::alphatWallFunction; Prt 0.85; value uniform 0;'))
    put('constant/thermophysicalProperties',header('thermophysicalProperties')+f'''
thermoType {{ type hePsiThermo; mixture pureMixture; transport const; thermo hConst;
 equationOfState perfectGas; specie specie; energy sensibleInternalEnergy; }}
mixture {{ specie {{ molWeight {c['molecularWeight']}; }}
 thermodynamics {{ Cp {c['Cp']}; Hf 0; }}
 transport {{ mu {c['mu']}; Pr {c['mu']*c['Cp']/c['conductivity']}; }} }}
''')
    put('constant/turbulenceProperties',header('turbulenceProperties')+'''
simulationType RAS;
RAS { RASModel SpalartAllmaras; turbulence on; printCoeffs on; }
''')
    put('system/fvSchemes',header('fvSchemes')+'''
fluxScheme Kurganov;
ddtSchemes { default localEuler; }
gradSchemes { default cellLimited Gauss linear 1; }
divSchemes { default none; div(tauMC) Gauss linear; div(phi,nuTilda) bounded Gauss upwind;
 div(((rho*nuEff)*dev2(T(grad(U))))) Gauss linear; }
laplacianSchemes { default Gauss linear limited 0.5; }
interpolationSchemes { default linear; reconstruct(rho) Minmod; reconstruct(U) Minmod; reconstruct(T) Minmod; }
snGradSchemes { default limited 0.5; }
wallDist { method meshWave; }
''')
    put('system/fvSolution',header('fvSolution')+'''
solvers {
 "(rho|rhoU|rhoE)" { solver diagonal; }
 "(U|e|nuTilda)" { solver smoothSolver; smoother symGaussSeidel; tolerance 1e-9; relTol 0; }
}
''')
    # OpenFOAM updates only this included numerical dictionary at checkpoints.
    # Physical data and the mesh are never changed by the startup schedule.
    for name,co,dt in [('startup',0.1,1e-7),('march',0.2,0.001)]:
        put('system/localTimeStep.'+name,f'maxCo {co}; maxDeltaT {dt}; rDeltaTSmoothingCoeff 0.5;\n')
    put('system/localTimeStep',(root/'system/localTimeStep.startup').read_text())
    put('system/controlDict',header('controlDict')+f'''
application vortiaCentralFoam;
startFrom latestTime; startTime 0; stopAt endTime; endTime {end_time};
deltaT 1; adjustTimeStep no;
#include "localTimeStep"
writeControl timeStep; writeInterval 1000; purgeWrite 3;
writeFormat ascii; writePrecision 9; writeCompression off; timeFormat general; timePrecision 9;
runTimeModifiable true;
maxDiffusionCo 0.1;
functions {{
 startupSchedule {{ type timeActivatedFileUpdate; libs (utilityFunctionObjects);
  fileToUpdate "<system>/localTimeStep";
  timeVsFile ((0 "<system>/localTimeStep.startup") (5000 "<system>/localTimeStep.march")); }}
 extrema {{ type fieldMinMax; libs (fieldFunctionObjects); fields (p T U nuTilda); mode magnitude;
  executeControl timeStep; executeInterval 100; writeControl timeStep; writeInterval 100; }}
 mass {{ type surfaceFieldValue; libs (fieldFunctionObjects); regionType patch; name inlet;
  operation sum; fields (phi); writeFields false;
  executeControl timeStep; executeInterval 100; writeControl timeStep; writeInterval 100; }}
 massOut {{ type surfaceFieldValue; libs (fieldFunctionObjects); regionType patch; name outlet;
  operation sum; fields (phi); writeFields false;
  executeControl timeStep; executeInterval 100; writeControl timeStep; writeInterval 100; }}
 massFar {{ type surfaceFieldValue; libs (fieldFunctionObjects); regionType patch; name farfield;
  operation sum; fields (phi); writeFields false;
  executeControl timeStep; executeInterval 100; writeControl timeStep; writeInterval 100; }}
 Mach {{ type MachNo; libs (fieldFunctionObjects); result Mach; executeControl writeTime; writeControl writeTime; }}
 wallResolution {{ type yPlus; libs (fieldFunctionObjects); executeControl writeTime; writeControl writeTime; }}
 profiles {{ type sets; libs (sampling); interpolationScheme cellPoint; setFormat csv;
  fields (U p T); writeControl writeTime;
  sets (
   exitProfile {{ type uniform; axis y; start ({xe:.12g} 0.0001 0); end ({xe:.12g} {c['outletHeight']-0.0001:.12g} 0); nPoints 161; }}
   axisProfile {{ type uniform; axis x; start (0.0001 0.0001 0); end ({xmax-0.0001:.12g} 0.0001 0); nPoints 501; }}
  ); }}
 finalMach {{ type MachNo; libs (fieldFunctionObjects); result Mach; executeControl onEnd; writeControl onEnd; }}
 finalFields {{ type writeObjects; libs (utilityFunctionObjects); objects (p U T rho nuTilda nut alphat Mach);
  writeOption anyWrite; writeControl onEnd; }}
}}
''')
    put('system/decomposeParDict',header('decomposeParDict')+'numberOfSubdomains 4;\nmethod scotch;\n')
    # A pressure ratio of 86 cannot be introduced into a cold, motionless cell
    # in one explicit step. This is an INITIAL GUESS, not a computed result.
    # The solver subsequently resolves viscous losses, shocks and entrainment.
    def area_mach(m):
        return (1/m)*(2/(gamma+1)*(1+(gamma-1)*m*m/2))**((gamma+1)/(2*(gamma-1)))
    def state_at(x):
        area=max(1.0,(wall(c,g,x)/c['throatHeight'])**2)
        m=bisect(lambda m:area_mach(m)-area,1e-5,1) if x<g['throatCenter'][0] else bisect(lambda m:area_mach(m)-area,1,10)
        t=c['inletTotalTemperature']/(1+(gamma-1)*m*m/2)
        p=p0*(t/c['inletTotalTemperature'])**(gamma/(gamma-1))
        u=m*math.sqrt(gamma*8314.462618/c['molecularWeight']*t)
        return p,t,u
    def fields(p,t,u):
        return f'volScalarFieldValue p {p:.12g} volScalarFieldValue T {t:.12g} volVectorFieldValue U ({u:.12g} 0 0) volScalarFieldValue nuTilda {nu(p,t):.12g}'
    init=header('setFieldsDict')+f'defaultFieldValues ({fields(pfar,c["exteriorTotalTemperature"],0)});\nregions\n(\n'
    tjet=c['inletTotalTemperature']*(pfar/p0)**((gamma-1)/gamma)
    ujet=math.sqrt(2*c['Cp']*(c['inletTotalTemperature']-tjet))
    # Smooth the starting jet/ambient contact over several radial cells.
    for j in range(30):
        y0=j*0.008; y1=(j+1)*0.008
        w=max(0.0,1-(((y0+y1)/2)/c['outletHeight'])**10) if y1<=c['outletHeight'] else 0.0
        t=c['exteriorTotalTemperature']+(tjet-c['exteriorTotalTemperature'])*w
        init+=f'boxToCell {{ box ({xe:.12g} {y0:.12g} -1) ({xmax:.12g} {y1:.12g} 1); fieldValues ({fields(pfar,t,ujet*w)}); }}\n'
    for j in range(240):
        x0=xe*j/240; x1=xe*(j+1)/240
        p,t,u=state_at((x0+x1)/2)
        init+=f'boxToCell {{ box ({x0:.12g} -1 -1) ({x1:.12g} {ymax:.12g} 1); fieldValues ({fields(p,t,u)}); }}\n'
    put('system/setFieldsDict',init+');\n')
    put('.vortia-nozzle-case','vortia-nozzle-video-2026-v2\n')
    put('README.md',f'''# Práctica de tobera — caso experimental OpenFOAM v2606

Estado: preparado; la ejecución y la convergencia deben comprobarse.

Los parámetros utilizados están en parameters.json (SI). La longitud ideal del dominio es {xmax:.6f} m y su radio exterior {ymax:.6f} m. Las alturas del perfil son radios desde el eje. Se retienen los radios de redondeo visibles al imponer las cotas finales; la reconstrucción se guarda en geometry/. El CAD original no fue proporcionado.

El gas ideal tiene Cp={c['Cp']:g} J/kg/K y masa molecular {c['molecularWeight']:g} kg/kmol (gamma={gamma:.8f}). La entrada usa presión y temperatura totales de {p0:g} Pa y {c['inletTotalTemperature']:g} K. El exterior usa {pfar:g} Pa y {ambient_t:g} K. La presión de operación es {c['operatingPressure']:g} Pa. La salida extrapola la presión donde el flujo normal sale supersónico; en las caras subsónicas impone la presión exterior. La relación de viscosidad turbulenta se aplica con la densidad local del gas entrante.

La malla estructurada es una cuña axisimétrica de 5 grados, concentrada en la pared y la capa de mezcla. Sustituye a los triángulos del vídeo. Los controles ampliados de malla pueden avisar del determinante pequeño de celdas muy anisótropas próximas a la pared; revisar también yPlus y la dependencia con el refinamiento.

Vortia construye el motor local de este caso con wmake en la primera ejecución. Se requiere OpenFOAM v2606 y su compilador. El código derivado de rhoCentralFoam y su licencia GPL se conservan en solver/. Utiliza Kurganov, reconstrucción Minmod, energía y Spalart–Allmaras. El límite por difusión y el rechazo de candidatos no físicos reducen el paso; no se recorta la temperatura. Es una aproximación numérica distinta del método estacionario implícito Roe de Fluent.

localEuler busca una solución estacionaria mediante pseudotiempo: las etiquetas son iteraciones, no segundos físicos. El límite solicitado es {end_time:g}; no es un criterio de convergencia. La inicialización cuasiunidimensional de setFields es una estimación, no un resultado CFD. Al reanudar una solución existente, Vortia conserva sus campos.

Los balances se guardan en postProcessing/mass, massOut y massFar; los perfiles de salida y del eje en postProcessing/profiles; el control de pared en postProcessing/wallResolution. Verificar positividad, balance global y variación entre estados antes de presentar resultados. Los residuos diagonales iguales a cero no acreditan convergencia.
''')
    return g


if __name__ == '__main__':
    p=argparse.ArgumentParser()
    p.add_argument('directory',type=Path)
    p.add_argument('--parameters',type=Path)
    p.add_argument('--refinement',type=int,default=1,choices=[1,2,3])
    p.add_argument('--end-time',type=float,default=50000, help='Pseudo-iteration limit (localEuler steady marching)')
    args=p.parse_args()
    c=DEFAULT.copy()
    if args.parameters: c.update(json.loads(args.parameters.read_text()))
    g=prepare(args.directory,c,args.refinement,args.end_time)
    print(json.dumps(g,indent=2))
