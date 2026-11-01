#!/usr/bin/env python3
"""Verify ELF dependency closure from APK bytes, never from build-tree paths.

Android API 26 public system DSOs are the only permitted external dependencies.
libc++_shared.so is deliberately NOT a system library and must be bundled if needed.
"""
import argparse
import re
import subprocess
import tempfile
import zipfile
from pathlib import Path

SYSTEM = frozenset({'libc.so', 'libm.so', 'libdl.so', 'liblog.so', 'libandroid.so',
                    'libz.so', 'libjnigraphics.so', 'libEGL.so', 'libGLESv1_CM.so',
                    'libGLESv2.so', 'libGLESv3.so', 'libOpenSLES.so', 'libOpenMAXAL.so',
                    'libmediandk.so', 'libvulkan.so', 'libaaudio.so', 'libcamera2ndk.so'})
ABIS = {'arm64-v8a': 'AArch64', 'x86_64': 'Advanced Micro Devices X86-64'}
REQUIRED = {'libeyeflow_jni.so', 'libeyeflow_core.so'}

def verify(apk, readelf):
    with zipfile.ZipFile(apk) as archive, tempfile.TemporaryDirectory() as tmp:
        names = archive.namelist()
        assert len(names) == len(set(names)), 'Duplicate ZIP entries in APK'
        packaged_abis = {n.split('/')[1] for n in names if n.startswith('lib/') and n.endswith('.so')}
        assert packaged_abis == set(ABIS), f'Unexpected ABI set: {packaged_abis}'
        for abi, machine in ABIS.items():
            entries = {Path(n).name: n for n in names if n.startswith(f'lib/{abi}/') and n.endswith('.so')}
            assert REQUIRED <= entries.keys(), f'{abi}: missing required runtime libraries: {REQUIRED - entries.keys()}'
            graph = {}
            for soname, entry in entries.items():
                assert entry == f'lib/{abi}/{soname}', f'Invalid library path: {entry}'
                path = Path(tmp) / abi / soname
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(archive.read(entry))
                out = subprocess.check_output([readelf, '-h', '-d', str(path)], text=True)
                assert re.search(r'Class:\s+ELF64', out), f'{entry}: expected ELF64'
                assert re.search(r'Machine:\s+' + re.escape(machine), out), f'{entry}: incorrect ELF machine'
                needed = re.findall(r'\(NEEDED\).*?\[(.*?)\]', out)
                actual_sonames = re.findall(r'\(SONAME\).*?\[(.*?)\]', out)
                assert actual_sonames == [soname], f'{entry}: incorrect/missing SONAME {actual_sonames}'
                assert all('/' not in d for d in needed), f'{entry}: path-valued DT_NEEDED {needed}'
                graph[soname] = needed
            seen = set()
            def visit(name):
                if name in seen: return
                seen.add(name)
                for dep in graph[name]:
                    assert dep in graph or dep in SYSTEM, f'{abi}: {name} needs missing {dep}'
                    if dep in graph: visit(dep)
            for name in graph: visit(name)
            assert 'libeyeflow_core.so' in graph['libeyeflow_jni.so'], f'{abi}: JNI bridge is not linked to core'
            for name, deps in sorted(graph.items()):
                print(f'{abi}/{name}: {", ".join(deps)}')
    print(f'PASS: actual APK recursive native dependency closure for both ABIs: {apk}')

if __name__ == '__main__':
    if not __debug__:
        raise RuntimeError("Run verification without Python optimization (-O/PYTHONOPTIMIZE)")
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('apk', type=Path)
    p.add_argument('--readelf', default='readelf')
    args = p.parse_args()
    verify(args.apk, args.readelf)
