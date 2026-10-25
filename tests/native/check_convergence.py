#!/usr/bin/env python3
"""Opt-in native OpenFOAM v2606 monitor regression, on private temporary cases.
Source OpenFOAM first; pass the compiled vortiaCentralFoam executable.
This verifies software decisions on a uniform equilibrium and a forced startup,
not the accuracy or completion of the user's nozzle assignment.
"""
import argparse,json,math,os,shutil,subprocess,tempfile
from pathlib import Path

HEADER='FoamFile {version 2.0; format ascii; class dictionary; object %s;}\n'
def run(tool,args,root):
    with (root/('log.'+Path(tool).name)).open('w') as log:
        subprocess.run([tool,*args],cwd=root,stdout=log,stderr=subprocess.STDOUT,check=True,
                       env={**os.environ,'OMPI_MCA_mpi_yield_when_idle':'1'})
def make(root):
    for name in ['0','system','constant']:(root/name).mkdir(parents=True)
    x=.2;y=.05*math.cos(math.radians(2.5));z=.05*math.sin(math.radians(2.5))
    points=[(0,0,0),(x,0,0),(x,y,-z),(0,y,-z),(0,0,0),(x,0,0),(x,y,z),(0,y,z)]
    (root/'system/blockMeshDict').write_text(HEADER%'blockMeshDict'+'convertToMeters 1;\nvertices ('+
        ' '.join('(%g %g %g)'%p for p in points)+''');
blocks (hex (0 1 2 3 4 5 6 7) (8 4 1) simpleGrading (1 1 1)); edges ();
boundary (
inlet {type patch; faces ((0 4 7 3));}
outlet {type patch; faces ((1 2 6 5));}
wall {type wall; faces ((3 7 6 2));}
wedgeFront {type wedge; faces ((0 3 2 1));}
wedgeBack {type wedge; faces ((4 5 6 7));}
axis {type empty; faces ((0 1 5 4));}
); mergePatchPairs ();
''')
    for field,dims,value in [('p','1 -1 -2 0 0 0 0','10000'),('T','0 0 0 1 0 0 0','300'),('U','0 1 -1 0 0 0 0','(0 0 0)')]:
        scalar=field!='U';bcs=''
        for name in ['inlet','outlet','wall','wedgeFront','wedgeBack','axis']:
            if name=='axis':bc='type empty;'
            elif name.startswith('wedge'):bc='type wedge;'
            elif field=='U':bc='type noSlip;' if name=='wall' else 'type zeroGradient;'
            else:bc='type fixedValue; value uniform '+value+';' if field=='p' and name in ['inlet','outlet'] else 'type zeroGradient;'
            bcs+=name+' { '+bc+' }\n'
        (root/'0'/field).write_text('FoamFile {version 2.0;format ascii;class vol'+('Scalar' if scalar else 'Vector')+'Field;object '+field+';}\ndimensions ['+dims+']; internalField uniform '+value+';boundaryField { '+bcs+'}\n')
    (root/'constant/thermophysicalProperties').write_text(HEADER%'thermophysicalProperties'+'''
thermoType {type hePsiThermo;mixture pureMixture;transport const;thermo hConst;equationOfState perfectGas;specie specie;energy sensibleInternalEnergy;}
mixture {specie {molWeight 24;} thermodynamics {Cp 1501.222417; Hf 0;} transport {mu 1.7894e-5; Pr .7;}}
''')
    (root/'constant/turbulenceProperties').write_text(HEADER%'turbulenceProperties'+'simulationType laminar;\n')
    (root/'system/fvSchemes').write_text(HEADER%'fvSchemes'+'''
fluxScheme Kurganov;ddtSchemes {default localEuler;}gradSchemes {default cellLimited Gauss linear 1;}
divSchemes {default none;div(tauMC) Gauss linear;div(((rho*nuEff)*dev2(T(grad(U))))) Gauss linear;}
laplacianSchemes {default Gauss linear limited .5;} interpolationSchemes {default linear;reconstruct(rho) Minmod;reconstruct(U) Minmod;reconstruct(T) Minmod;}
snGradSchemes {default limited .5;}wallDist {method meshWave;}
''')
    (root/'system/fvSolution').write_text(HEADER%'fvSolution'+'''
solvers {"(rho|rhoU|rhoE)" {solver diagonal;} "(U|e)" {solver smoothSolver;smoother symGaussSeidel;tolerance 1e-9;relTol 0;}}
''')
    (root/'system/controlDict').write_text(HEADER%'controlDict'+'''
application vortiaCentralFoam; noSwirl true; startFrom latestTime;startTime 0;stopAt endTime;endTime 1000;deltaT 1;
adjustTimeStep no;maxCo .05;maxDeltaT 1e-6;rDeltaTSmoothingCoeff .5;maxDiffusionCo .1;
writeControl timeStep;writeInterval 1000;purgeWrite 0;writeFormat ascii;writePrecision 12;
writeCompression off;timeFormat general;timePrecision 9;runTimeModifiable true;
''')
    run('blockMesh',[],root);run('checkMesh',[],root)
def check(root,expected,iteration):
    report=json.loads((root/'vortia-convergence.json').read_text())
    assert report['status']==expected,report
    assert report['iteration']==iteration,report
    fieldroot=root/'processor0' if (root/'processor0').exists() else root
    assert (fieldroot/str(iteration)/'p').is_file(), 'Actual final checkpoint was not saved'
    return report

def main():
    p=argparse.ArgumentParser();p.add_argument('--solver',type=Path,required=True);p.add_argument('--output',type=Path,required=True);args=p.parse_args()
    args.output.mkdir(parents=True,exist_ok=False)
    base=args.output/'equilibrium';make(base)
    parallel=args.output/'parallel';shutil.copytree(base,parallel)
    forced=args.output/'forced';shutil.copytree(base,forced)
    run(str(args.solver),[],base);a=check(base,'converged',700)
    (parallel/'system/decomposeParDict').write_text(HEADER%'decomposeParDict'+'numberOfSubdomains 4;method scotch;\n')
    run('decomposePar',[],parallel);run('mpirun',['-np','4',str(args.solver),'-parallel'],parallel);b=check(parallel,'converged',700)
    run('reconstructPar',['-latestTime'],parallel)
    # A boundary pressure transient must not pass despite zero diagonal residuals.
    path=forced/'0/p';path.write_text(path.read_text().replace('inlet { type fixedValue; value uniform 10000;', 'inlet { type fixedValue; value uniform 3640000;'))
    ctrl=forced/'system/controlDict';ctrl.write_text(ctrl.read_text().replace('endTime 1000;','endTime 751;'))
    run(str(args.solver),[],forced);c=check(forced,'limit',751)
    assert c['sampled'] and (c['localMass']>c['massTolerance'] or c['fieldChange']>c['fieldTolerance']),c
    # Nonzero uniform Euler flow must retain tangential wall velocity.
    inviscid=args.output/'inviscid';make(inviscid)
    thermo=inviscid/'constant/thermophysicalProperties';thermo.write_text(thermo.read_text().replace('mu 1.7894e-5;','mu 0;'))
    velocity=inviscid/'0/U';velocity.write_text(velocity.read_text().replace('uniform (0 0 0)','uniform (100 0 0)').replace('wall { type noSlip;','wall { type slip;'))
    run(str(args.solver),[],inviscid);d=check(inviscid,'converged',700)
    text=(inviscid/'700/U').read_text();assert 'type            slip;' in text or 'type slip;' in text
    from compare_conservative_updates import values
    u=values(inviscid/'700/U');assert max(abs(x-100) for x in u[::3])<1e-7
    assert max(abs(x) for x in u[1::3])<1e-7 and max(abs(x) for x in u[2::3])==0
    (args.output/'summary.json').write_text(json.dumps(dict(equilibrium=a,parallel=b,forced=c,inviscid=d),indent=2)+'\n')
    print(args.output/'summary.json')
if __name__=='__main__':main()
