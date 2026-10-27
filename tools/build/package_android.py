"""Package unmodified official NSH Java with app-owned Android adapters, then sign it."""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile

from build_android_bridge import build, platform_jar, sdk_directory
from android_manifest import component_manifest

NATIVE_INIT = b'Java_com_manonloki_lokimetrisdisplay_bridge_DisplayApplication_nativeInit'


def has_native_init(data):
    """Check a defined, visible JNI function in ELF .dynsym, never a debug-string occurrence."""
    import struct
    if len(data) < 64 or data[:4] != b'\x7fELF' or data[5] != 1 or data[4] not in {1, 2}:
        return False
    wide = data[4] == 2
    try:
        section_offset = struct.unpack_from('<Q' if wide else '<I', data, 40 if wide else 32)[0]
        stride, count = struct.unpack_from('<HH', data, 58 if wide else 46)
        minimum = 64 if wide else 40
        if stride < minimum or count == 0 or section_offset + stride * count > len(data):
            return False
        def section(index):
            if index >= count:
                raise ValueError('ELF string-table section outside header table')
            offset = section_offset + index * stride
            kind = struct.unpack_from('<I', data, offset + 4)[0]
            start, size = struct.unpack_from('<QQ' if wide else '<II', data, offset + (24 if wide else 16))
            link = struct.unpack_from('<I', data, offset + (40 if wide else 24))[0]
            entry = struct.unpack_from('<Q' if wide else '<I', data, offset + (56 if wide else 36))[0]
            if start + size > len(data):
                raise ValueError('ELF section outside library')
            return kind, start, size, link, entry
        for index in range(count):
            kind, start, size, link, entry = section(index)
            if kind != 11 or entry < (24 if wide else 16):
                continue
            string_kind, string_start, string_size, _, _ = section(link)
            if string_kind != 3:
                continue
            names = data[string_start:string_start + string_size]
            for offset in range(start, start + size - entry + 1, entry):
                name = struct.unpack_from('<I', data, offset)[0]
                info, visibility, defined = struct.unpack_from('<BBH', data, offset + (4 if wide else 12))
                if defined == 0 or info >> 4 not in {1, 2} or info & 15 != 2 or visibility & 3 not in {0, 3}:
                    continue
                end = names.find(b'\0', name)
                if name < len(names) and end >= 0 and names[name:end] == NATIVE_INIT:
                    return True
    except (IndexError, ValueError, struct.error):
        return False
    return False


def verify_jni_exports(apk):
    abis = []
    with zipfile.ZipFile(apk) as archive:
        libraries = [name for name in archive.namelist() if name.startswith('lib/') and name.endswith('/libnsh.so')]
        if not libraries:
            raise RuntimeError('APK has no app-owned libnsh.so runtime')
        for name in libraries:
            if not has_native_init(archive.read(name)):
                raise RuntimeError('Android runtime lacks exported ' + NATIVE_INIT.decode() + ': ' + name
                                   + '; rebuild the app runtime before packaging its Java bridge')
            abis.append(name.split('/')[1])
    return sorted(abis)


def update_java_and_sign(apk, signing, env, log):
    # A stale official/previous runtime cannot initialize this app's new Application.
    verify_jni_exports(apk)
    sdk = sdk_directory(None)
    versions = [path for path in (sdk / 'build-tools').iterdir() if (path / 'lib/d8.jar').is_file()]
    if not versions:
        raise RuntimeError('An existing Android build-tools installation with D8 is required')
    tools = max(versions, key=lambda path: tuple(int(part) for part in path.name.split('.') if part.isdigit()))
    java = Path(env['JAVA_HOME']) / 'bin/java' if env.get('JAVA_HOME') else Path(shutil.which('java') or '')
    if not java.is_file():
        raise RuntimeError('An existing JDK is required for D8 and APK signing')
    platform = platform_jar(sdk, 37)
    with tempfile.TemporaryDirectory(prefix='nsh-apk-', dir=apk.parent) as temporary:
        stage = Path(temporary)
        build(argparse.Namespace(out=stage / 'java', sdk=sdk, compile_sdk=37, javac=None))
        dex = stage / 'dex'; dex.mkdir()
        subprocess.run([str(java), '-cp', str(tools / 'lib/d8.jar'), 'com.android.tools.r8.D8',
                        '--lib', str(platform), '--min-api', '28', '--output', str(dex),
                        str(stage / 'java/classes.jar')], check=True, stdout=log, stderr=subprocess.STDOUT)
        unsigned = stage / 'unsigned.apk'
        with zipfile.ZipFile(apk) as original, zipfile.ZipFile(unsigned, 'w') as output:
            for entry in original.infolist():
                if entry.filename.startswith('META-INF/') or (entry.filename.startswith('classes') and entry.filename.endswith('.dex')):
                    continue
                if entry.filename == 'AndroidManifest.xml':
                    manifest, target_sdk = component_manifest(original.read(entry.filename))
                    if target_sdk is not None and target_sdk >= 37:
                        raise RuntimeError('Target SDK 37+ requires explicit local-network runtime permission support')
                    output.writestr(entry, manifest)
                else:
                    output.writestr(entry, original.read(entry.filename))
            for path in sorted(dex.glob('*.dex')):
                output.write(path, path.name, compress_type=zipfile.ZIP_DEFLATED)
        aligned = stage / 'aligned.apk'
        subprocess.run([str(tools / 'zipalign'), '-f', '-P', '16', '4', str(unsigned), str(aligned)],
                       check=True, stdout=log, stderr=subprocess.STDOUT)
        keystore = signing[signing.index('--keystore') + 1]
        alias = signing[signing.index('--key-alias') + 1]
        signer = [str(java), '-jar', str(tools / 'lib/apksigner.jar')]
        subprocess.run(signer + ['sign', '--ks', str(keystore), '--ks-key-alias', str(alias),
                                '--ks-pass', 'env:NSH_KEYSTORE_PASSWORD', '--key-pass', 'env:LOKI_SIGNING_KEY_PASSWORD',
                                '--out', str(apk), str(aligned)], env=env, check=True,
                       stdout=log, stderr=subprocess.STDOUT)
        subprocess.run(signer + ['verify', '--verbose', str(apk)], check=True,
                       stdout=log, stderr=subprocess.STDOUT)
        # The CLI's v4 sidecar describes the APK before DEX/manifest replacement.
        apk.with_suffix(apk.suffix + '.idsig').unlink(missing_ok=True)
        shutil.copy2(stage / 'java/java-build.json', apk.parent / 'java-build.json')
        return target_sdk
