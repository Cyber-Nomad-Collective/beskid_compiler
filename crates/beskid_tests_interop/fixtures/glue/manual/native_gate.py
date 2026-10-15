#!/usr/bin/env python3
"""Retain real manual import/export gates. Never qualifies generated Glue or release."""
import argparse, hashlib, json, os, pathlib, re, shutil, subprocess, sys
p=argparse.ArgumentParser()
p.add_argument('--candidate', required=True); p.add_argument('--runtime-prefix', required=True)
p.add_argument('--provider-validator', required=True)
p.add_argument('--corelib-root', required=True); p.add_argument('--output', required=True)
a=p.parse_args(); source=pathlib.Path(__file__).resolve().parent
output=pathlib.Path(a.output).resolve(); output.mkdir(parents=True,exist_ok=True)
candidate=pathlib.Path(a.candidate).resolve(); kit=pathlib.Path(a.runtime_prefix).resolve()
corelib=pathlib.Path(a.corelib_root).resolve()
env=os.environ.copy(); env.update(BESKID_HOME=str(output/'home'), BESKID_CONFIG_DIR=str(output/'config'), BESKID_CORELIB_ROOT=str(corelib), BESKID_RUNTIME_PREFIX=str(kit), BESKID_RUNTIME_KIT_PROFILE='debug')
commands=[]
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def run(argv,label,timeout=900):
    executable=pathlib.Path(shutil.which(str(argv[0])) or argv[0]).absolute()
    argv=[str(executable),*map(str,argv[1:])]
    try:
        c=subprocess.run(argv,cwd=output,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=timeout)
        code=c.returncode; stdout=c.stdout; stderr=c.stderr
    except subprocess.TimeoutExpired as e:
        code=124; stdout=e.stdout or b''; stderr=(e.stderr or b'')+b'\nfixture command deadline expired\n'
    out=output/(label+'.stdout'); err=output/(label+'.stderr'); out.write_bytes(stdout); err.write_bytes(stderr)
    commands.append(dict(label=label,argv=argv,exit_code=code,tool_sha256=sha(executable),stdout=dict(path=out.name,sha256=sha(out)),stderr=dict(path=err.name,sha256=sha(err))))
    return code,stdout
provider_status,_=run([sys.executable,source/'provider_gate.py','--validator',a.provider_validator,'--runtime-prefix',kit,'--output',output/'provider-safety'],'canonical-provider-safety',900)
provider_fixture_pass=provider_status==0
# Rust-side preproof stays separate and is not used as import/export success.
preproof,_=run([sys.executable, source/'build.py','--output',output/'shim'],'rust-preproof')
env['PATH']=str(output/'shim')+os.pathsep+env.get('PATH','')
env['LD_LIBRARY_PATH']=str(output/'shim')+os.pathsep+env.get('LD_LIBRARY_PATH','')
env['DYLD_LIBRARY_PATH']=str(output/'shim')+os.pathsep+env.get('DYLD_LIBRARY_PATH','')
project=output/'project'; project.mkdir(exist_ok=True); shutil.copytree(source/'src',project/'src',dirs_exist_ok=True)
manifest=(source/'manual_tests.bproj').read_text().replace('libraries = [glue_manual]', 'libraries = [glue_manual] searchPaths = ['+json.dumps(str(output/'shim'))+']')
(project/'manual_tests.bproj').write_text(manifest)
expected=re.findall(r'^test\s+(\w+)',(source/'src'/'ManualImportTests.bd').read_text(),re.M)
import_code,raw=run([candidate,'test','--plain','--json','--project',project/'manual_tests.bproj','--target','ManualImportTests','--target-timeout','120'],'beskid-import')
import_pass=False
try:
    # CLI progress may precede the final pretty JSON object.
    for i,b in enumerate(raw):
        if b!=123: continue
        try: result=json.loads(raw[i:]); break
        except (json.JSONDecodeError,UnicodeDecodeError): pass
    summary=result['summary']; names=[t['name'] for t in result['tests']]
    import_pass=import_code==0 and summary['passed']==len(expected) and all(summary[k]==0 for k in ['failed','skipped','timed_out']) and sorted(names)==sorted(expected)
except (KeyError,NameError,TypeError): pass
windows=sys.platform=='win32'; mac=sys.platform=='darwin'
lib=output/('beskid_manual.dll' if windows else 'libbeskid_manual.dylib' if mac else 'libbeskid_manual.so')
export_code,_=run([candidate,'build','--plain','--project',project/'manual_tests.bproj','--target','ManualExports','--kind','shared','--prefer-dynamic','--output',lib],'beskid-export-build')
# Caller recipes consume the actual emitted library and canonical runtime archive.
# A missing owned-release symbol or normalized managed entry is a real link RED.
providers=[f for f in (kit/'lib'/'beskid-runtime'/'abi-5').rglob('*') if f.is_file() and f.parent.name=='shared' and f.name in {'libbeskid_runtime.dylib','libbeskid_runtime.so','beskid_runtime.lib','beskid_runtime.dll.lib'}]
caller_pass=False
shared_provider_pass=False
if export_code==0 and len(providers)==1:
    dependency_tool=['dumpbin','/dependents',lib] if windows else ['otool','-L',lib] if mac else ['readelf','-d',lib]
    dependency_code,dependencies=run(dependency_tool,'beskid-export-runtime-dependencies',60)
    expected_provider='beskid_runtime.dll' if windows else providers[0].name
    shared_provider_pass=dependency_code==0 and expected_provider.encode() in dependencies
if export_code==0 and len(providers)==1 and shared_provider_pass:
    abi_include=source.parents[3]/'beskid_abi'/'include'
    bootstrap=output/('export_bootstrap.obj' if windows else 'export_bootstrap.o')
    compiler=['cl','/nologo','/MD','/c',source/'export_bootstrap.c','/I'+str(abi_include),'/Fo:'+str(bootstrap)] if windows else ['cc','-std=c11','-c',source/'export_bootstrap.c','-I'+str(abi_include),'-o',bootstrap]
    code,_=run(compiler,'export-bootstrap-build')
    if code==0:
        caller=output/('export_caller.exe' if windows else 'export_caller')
        linklib=output/'beskid_manual.dll.lib' if windows else lib
        argv=['rustc','--edition','2024',source/'export_caller.rs','-o',caller,'-C','link-arg='+str(bootstrap),'-C','link-arg='+str(linklib),'-C','link-arg='+str(providers[0])]
        if not windows: argv+=['-C','link-arg=-Wl,-rpath,'+str(output),'-C','link-arg=-Wl,-rpath,'+str(providers[0].parent)]
        else: env['PATH']=str(providers[0].parent)+os.pathsep+env.get('PATH','')
        code,_=run(argv,'rust-export-caller-build')
        if code==0: code,_=run([caller],'rust-export-caller-run',120)
        caller_pass=code==0
receipt=dict(schema_version=1,scope='actual-manual-beskid-rust-gates',candidate_sha256=sha(candidate),runtime_files={str(f.relative_to(kit)):sha(f) for f in kit.rglob('*') if f.is_file()},sources={str(f.relative_to(source)):sha(f) for f in source.rglob('*') if f.is_file() and 'obj' not in f.parts and '__pycache__' not in f.parts},commands=commands,expected_import_tests=expected,import_pass=import_pass,export_caller_pass=caller_pass,shared_provider_pass=shared_provider_pass,canonical_provider_fixture_pass=provider_fixture_pass,rust_preproof_pass=preproof==0,generated_glue_qualified=False,release_qualified=False)
(output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
raise SystemExit(0 if import_pass and caller_pass and provider_fixture_pass else 1)
