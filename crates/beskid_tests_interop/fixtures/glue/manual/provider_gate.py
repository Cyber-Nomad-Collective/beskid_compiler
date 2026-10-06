#!/usr/bin/env python3
"""Execute real safety fixtures only against a current source-validated provider."""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, sys
p=argparse.ArgumentParser()
p.add_argument('--validator',required=True); p.add_argument('--runtime-prefix',required=True)
p.add_argument('--output',required=True)
a=p.parse_args(); source=pathlib.Path(__file__).resolve().parent
compiler=source.parents[4]; output=pathlib.Path(a.output).resolve(); output.mkdir(parents=True,exist_ok=True)
if (output/'receipt.json').exists(): raise SystemExit('output already contains a retained fixture receipt; use a fresh directory')
commands=[]; env=os.environ.copy()
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def run(argv,label,timeout=120):
    tool=pathlib.Path(shutil.which(str(argv[0])) or argv[0]).resolve(strict=True)
    before=sha(tool); argv=[str(tool),*map(str,argv[1:])]
    stdout=output/(label+'.stdout'); stderr=output/(label+'.stderr')
    with stdout.open('wb') as out, stderr.open('wb') as err:
        try: code=subprocess.run(argv,cwd=output,env=env,stdout=out,stderr=err,timeout=timeout).returncode
        except subprocess.TimeoutExpired: code=124
    if sha(tool)!=before: code=125
    if stdout.stat().st_size>1048576 or stderr.stat().st_size>1048576: code=126
    commands.append(dict(label=label,argv=argv,exit_code=code,tool_sha256=before,
        stdout=dict(path=stdout.name,sha256=sha(stdout)),stderr=dict(path=stderr.name,sha256=sha(stderr))))
    return code,stdout
status,raw=run([a.validator,a.runtime_prefix],'current-canonical-provider')
qualified=[]; provider=None
if status==0:
    packet=json.loads(raw.read_text()); assert packet['schema_version']==1 and packet['runtime_sources_current']
    assert packet['scope']=='canonical-provider-fixture-inputs' and not packet['release_qualified']
    target=packet['target']; windows=target=='x86_64-pc-windows-msvc'; mac=target=='aarch64-apple-darwin'
    assert target in ['x86_64-unknown-linux-gnu','aarch64-apple-darwin','x86_64-pc-windows-msvc']
    abi=compiler/'crates'/'beskid_abi'/'include'; owner=compiler/'runtime'/'Glue'
    assert sha(abi/'beskid_runtime_abi_v5.h')==packet['abi_header_sha256']
    provider=pathlib.Path(packet['shared_library']).resolve(strict=True)
    root=pathlib.Path(packet['kit_root']).resolve(strict=True)
    assert provider.is_relative_to(root)
    assert sha(provider)==packet['manifest']['shared_library']['sha256']
    issuer=hashlib.sha256(b'beskid.Glue.OwnerIssuer.V1\0')
    for name in ['owner_identity_v1.c','owner_identity_v1.h']:
        name_bytes=name.encode(); data=(owner/name).read_bytes()
        issuer.update(len(name_bytes).to_bytes(8,'little')); issuer.update(name_bytes)
        issuer.update(len(data).to_bytes(8,'little')); issuer.update(data)
    assert issuer.hexdigest()==packet['manifest']['issuer_source_sha256']
    link=pathlib.Path(packet['link_library']).resolve(strict=True); assert link.is_relative_to(root)
    if windows: env['PATH']=str(provider.parent)+os.pathsep+env.get('PATH','')
    for fixture in ['owner_image_closure_tests.c','managed_input_root_tests.c']:
        name=pathlib.Path(fixture).stem; executable=output/(name+('.exe' if windows else ''))
        argv=['cl','/nologo','/std:c11',source/fixture,'/I'+str(abi),'/I'+str(owner),'/Fe:'+str(executable),'/link',link] if windows else ['cc','-std=c11',source/fixture,'-I'+str(abi),'-I'+str(owner),link,'-Wl,-rpath,'+str(provider.parent),'-o',executable]
        code,_=run(argv,name+'-build')
        if code==0:
            inspect=['dumpbin','/dependents',executable] if windows else ['otool','-L',executable] if mac else ['readelf','-d',executable]
            code,dependencies=run(inspect,name+'-shared-dependency')
            if code==0 and provider.name.encode() not in dependencies.read_bytes(): code=127
        if code==0: code,_=run([executable],name+'-run')
        qualified.append(dict(source=fixture,source_sha256=sha(source/fixture),passed=code==0,
            executable_sha256=sha(executable) if executable.is_file() else None))
    assert sha(provider)==packet['manifest']['shared_library']['sha256']
receipt=dict(schema_version=1,scope='actual-canonical-provider-safety-fixtures',commands=commands,
    fixtures=qualified,provider_sha256=sha(provider) if provider else None,
    sources={name:sha(source/name) for name in ['provider_gate.py','owner_image_closure_tests.c','managed_input_root_tests.c']},
    resolver_source_sha256=sha(compiler/'crates'/'beskid_abi'/'examples'/'resolve_glue_fixture_provider.rs'),
    canonical_header_sha256=sha(compiler/'crates'/'beskid_abi'/'include'/'beskid_runtime_abi_v5.h'),
    owner_sources={name:sha(compiler/'runtime'/'Glue'/name) for name in ['owner_identity_v1.c','owner_identity_v1.h']},
    candidate_current_provider=status==0,release_qualified=False)
(output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
raise SystemExit(0 if status==0 and len(qualified)==2 and all(f['passed'] for f in qualified) else 1)
