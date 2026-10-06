#!/usr/bin/env python3
"""Source-bound native preproof. Exit success never qualifies Beskid Glue."""
import argparse, hashlib, json, pathlib, platform, subprocess, shutil, os
p=argparse.ArgumentParser(); p.add_argument('--output',required=True); p.add_argument('--candidate'); args=p.parse_args()
source=pathlib.Path(__file__).resolve().parent; output=pathlib.Path(args.output).resolve(); output.mkdir(parents=True,exist_ok=True)
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
environment=os.environ.copy()
environment.update(BESKID_HOME=str(output/"home"), BESKID_CONFIG_DIR=str(output/"config"))
commands=[]
tools={}
def run(argv, label):
    executable=pathlib.Path(shutil.which(argv[0]) or argv[0]).absolute()
    if not executable.is_file(): raise SystemExit('missing required tool '+str(executable))
    tools[str(executable)]=sha(executable)
    argv=[str(executable),*argv[1:]]
    completed=subprocess.run(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60,env=environment)
    out=output/(label+'.stdout'); err=output/(label+'.stderr'); out.write_bytes(completed.stdout); err.write_bytes(completed.stderr)
    commands.append(dict(argv=argv,exit_code=completed.returncode,stdout=dict(path=out.name,sha256=sha(out)),stderr=dict(path=err.name,sha256=sha(err))))
    return completed
host=run(['rustc','-vV'],'rustc-version').stdout.decode(); target=next(line[6:] for line in host.splitlines() if line.startswith('host: '))
allowed=['x86_64-unknown-linux-gnu','aarch64-apple-darwin','x86_64-pc-windows-msvc']
if target not in allowed: raise SystemExit('unsupported native release target '+target)
windows=platform.system()=='Windows'; mac=platform.system()=='Darwin'
lib=output/('glue_manual.dll' if windows else 'libglue_manual.dylib' if mac else 'libglue_manual.so'); tests=output/('selfcheck.exe' if windows else 'selfcheck'); caller=output/('caller.exe' if windows else 'caller')
status=run(['rustc','--edition','2024','--test',str(source/'selfcheck.rs'),'-o',str(tests)],'selfcheck-build').returncode
if status==0: status=run([str(tests),'--test-threads=1'],'selfcheck-run').returncode
if status==0: status=run(['rustc','--edition','2024','--crate-type','cdylib',str(source/'native.rs'),'-o',str(lib)],'shim-build').returncode
# MSVC requires its native environment: no GCC fallback to a different ABI.
if status==0:
    argv=['cl','/nologo',str(source/'caller.c'),'/Fe:'+str(caller),'/link','/LIBPATH:'+str(output),'glue_manual.dll.lib'] if windows else ['cc','-std=c11',str(source/'caller.c'),'-L'+str(output),'-lglue_manual','-Wl,-rpath,'+str(output),'-o',str(caller)]
    status=run(argv,'c-caller-build').returncode
if status==0: status=run([str(caller)],'c-caller-run').returncode
checks=[]
if args.candidate:
    candidate=pathlib.Path(args.candidate).resolve()
    if not environment.get("BESKID_CORELIB_ROOT"):
        raise SystemExit("candidate checks require explicit BESKID_CORELIB_ROOT")
    for name in ['PrimitivePrerequisite.bd','ManualImport.bd','ManualExport.bd','ManualConsumer.bd']:
        completed=run([str(candidate),'check','--plain',str(source/name)],'beskid-'+name)
        checks.append(dict(source=name,exit_code=completed.returncode,candidate_sha256=sha(candidate)))
receipt=dict(schema_version=1,scope='rust-c-abi-native-preproof-only',target=target,rust_only_status='passed' if status==0 else 'failed',beskid_glue_qualified=False,commands=commands,tools=tools,beskid_checks=checks,sources={f.name:sha(f) for f in source.iterdir() if f.is_file()},artifacts={f.name:sha(f) for f in [lib,tests,caller] if f.is_file()})
(output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
raise SystemExit(status)
