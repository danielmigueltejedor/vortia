#!/usr/bin/env python3
"""Private A/B benchmark of the same localEuler equations, retaining both cases.
Requires sourced OpenFOAM v2606 and an explicitly selected read-only seed.
Stops its own run if a different native CFD solver starts on this desktop.
"""
import argparse,json,os,re,shutil,signal,subprocess,time
from pathlib import Path

def other_solver(binary):
    rows=subprocess.check_output(['ps','-axo','comm='],text=True).splitlines()
    return any(Path(x.strip()).name.startswith('vortiaCentralFoam') and Path(x.strip()).name!=binary.name for x in rows)
def command(args,root,name,binary):
    if other_solver(binary):raise RuntimeError('Another CFD calculation is running; benchmark deferred')
    with (root/name).open('w') as out:
        started=time.perf_counter();p=subprocess.Popen(args,cwd=root,stdout=out,stderr=subprocess.STDOUT,start_new_session=True,
            env={**os.environ,'OMPI_MCA_mpi_yield_when_idle':'1'})
        while p.poll() is None:
            if other_solver(binary):
                os.killpg(p.pid,signal.SIGTERM);p.wait();raise RuntimeError('Stopped private benchmark to free the user CPU')
            time.sleep(.2)
        if p.returncode:raise RuntimeError(str(args)+' failed; see '+str(root/name))
    return time.perf_counter()-started

def values(path):
    text=path.read_text();m=re.search(r'internalField\s+nonuniform\s+List<\w+>\s+\d+\s*\((.*?)\n\)',text,re.S)
    if not m:raise RuntimeError('Expected nonuniform saved field '+str(path))
    return [float(x) for x in re.findall(r'[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?',m[1])]
def main():
    p=argparse.ArgumentParser();p.add_argument('--seed',type=Path,required=True);p.add_argument('--start',default='1000');p.add_argument('--solver',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--steps',type=int,default=200);p.add_argument('--variant',choices=['direct','quiet','combined'],default='quiet');p.add_argument('--ranks',type=int,default=1);args=p.parse_args()
    args.output.mkdir(exist_ok=False,parents=True);end=int(args.start)+args.steps;reports=[];roots={}
    for index,mode in enumerate(['reference','direct','direct','reference']):
        root=args.output/f'{index}-{mode}';root.mkdir()
        for name in ['constant','system',args.start]:shutil.copytree(args.seed/name,root/name)
        if (args.seed/'dynamicCode').is_dir():shutil.copytree(args.seed/'dynamicCode',root/'dynamicCode')
        controls={'endTime':end,'writeInterval':args.steps,'writePrecision':12,'directConservativeUpdates':str(mode=='direct').lower(),'reuseStepSnapshots':str(mode=='direct' and args.variant=='combined').lower(),'vortiaLogInterval':10 if mode=='direct' and args.variant!='direct' else 1,'vortiaConvergence':'{ enabled false; }'}
        for key,value in controls.items():subprocess.run(['foamDictionary','system/controlDict','-entry',key,'-set',str(value)],cwd=root,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        if args.ranks>1:
            (root/'system/decomposeParDict').write_text('FoamFile {version 2.0;format ascii;class dictionary;object decomposeParDict;}\nnumberOfSubdomains '+str(args.ranks)+';method scotch;\n')
            if index==0:command(['decomposePar','-latestTime'],root,'log.decompose',args.solver)
            else:
                # Scotch can choose a different partition on each invocation.
                # Freeze both cell addressing and initial fields for a fair A/B.
                for processor in roots['reference'].glob('processor[0-9]*'):
                    destination=root/processor.name;destination.mkdir()
                    for name in ['constant',args.start]:shutil.copytree(processor/name,destination/name)
        commandline=[str(args.solver)] if args.ranks==1 else ['mpirun','-np',str(args.ranks),str(args.solver),'-parallel']
        elapsed=command(commandline,root,'log.solver',args.solver)
        if args.ranks>1:command(['reconstructPar','-latestTime'],root,'log.reconstruct',args.solver)
        marker=re.findall(r'^VORTIA_TIMING (.+)$',(root/'log.solver').read_text(),re.M)
        compute=json.loads(marker[-1])['computeSeconds'] if marker else None
        reports.append(dict(mode=mode,seconds=elapsed,computeSeconds=compute,case=str(root)));roots.setdefault(mode,root)
    comparison={}
    for field in ['p','T','rho','U','nuTilda']:
        a=values(roots['reference']/str(end)/field);b=values(roots['direct']/str(end)/field);assert len(a)==len(b)
        error=max(abs(x-y)/max(abs(x),abs(y),1) for x,y in zip(a,b));comparison[field]=error
        if error>1e-9:raise RuntimeError('Field mismatch: '+field+' relative error '+str(error))
        if field in ['p','T','rho']:assert min(b)>0
        if field=='U':assert max(abs(x) for x in b[2::3])==0
    result=dict(steps=args.steps,ranks=args.ranks,runs=reports,maximumScaledFieldErrors=comparison,
        referenceMeanSeconds=sum(x['seconds'] for x in reports if x['mode']=='reference')/2,
        directMeanSeconds=sum(x['seconds'] for x in reports if x['mode']=='direct')/2)
    if all(x['computeSeconds'] is not None for x in reports):
        result['computeSpeedup']=sum(x['computeSeconds'] for x in reports if x['mode']=='reference')/sum(x['computeSeconds'] for x in reports if x['mode']=='direct')
    result['variant']=args.variant
    result['speedup']=result['referenceMeanSeconds']/result['directMeanSeconds']
    (args.output/'summary.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
if __name__=='__main__':main()
