#!/usr/bin/env python3
"""Regression-test the packaging gate with mutations of a known-good APK."""
import argparse
import importlib.util
import tempfile
import zipfile
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('apk', type=Path)
p.add_argument('--readelf', default='readelf')
a = p.parse_args()
spec = importlib.util.spec_from_file_location('verify', Path(__file__).with_name('verify-apk-native.py'))
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)
v.verify(a.apk, a.readelf)
with zipfile.ZipFile(a.apk) as z:
    originals = {n: z.read(n) for n in z.namelist()}
with tempfile.TemporaryDirectory() as tmp:
    for abi in v.ABIS:
        core = f'lib/{abi}/libeyeflow_core.so'
        for case in ('missing-core', 'missing-transitive-dependency'):
            data = dict(originals)
            if case == 'missing-core':
                del data[core]
            else:
                # Same-length name preserves the ELF layout; the JNI bridge still
                # points to its present core, whose own dependency is now missing.
                if b'libdl.so\0' not in data[core]:
                    raise RuntimeError('Test fixture has no libdl.so dependency')
                data[core] = data[core].replace(b'libdl.so\0', b'libzz.so\0')
            apk = Path(tmp) / f'{abi}-{case}.apk'
            with zipfile.ZipFile(apk, 'w') as z:
                for name, content in data.items(): z.writestr(name, content)
            try:
                v.verify(apk, a.readelf)
            except AssertionError as e:
                expected = 'missing required runtime' if case == 'missing-core' else 'needs missing libzz.so'
                if expected not in str(e): raise
                print(f'PASS: rejects {abi} {case}: {e}')
            else:
                raise RuntimeError(f'Gate accepted broken APK: {abi} {case}')
