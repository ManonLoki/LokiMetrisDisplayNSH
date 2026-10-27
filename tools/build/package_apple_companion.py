#!/usr/bin/env python3
"""Archive a prelinked NSH iPhone app with its independent Watch target."""
import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import shutil
import struct
import subprocess
import tempfile
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[2]
WATCH_PROJECT = ROOT / 'platform/watchos/LokiMetrisWatch.xcodeproj'
WIDGET_PROJECT = ROOT / 'platform/ios/widgets/LokiMetrisWidgets.xcodeproj'


def run(command, *, log=None):
    subprocess.run([str(value) for value in command], check=True, cwd=ROOT,
                   stdout=log, stderr=subprocess.STDOUT if log else None)


def identifier(name):
    return hashlib.sha256(name.encode()).hexdigest()[:24].upper()


def project_value(value):
    if isinstance(value, dict):
        return '{' + ''.join(json.dumps(str(key), ensure_ascii=False) + ' = ' + project_value(item) + ';'
                             for key, item in value.items()) + '}'
    if isinstance(value, list):
        return '(' + ','.join(project_value(item) for item in value) + ')'
    if isinstance(value, int):
        return str(value)
    return json.dumps(str(value), ensure_ascii=False)


def native_target(project, name):
    data = subprocess.check_output(['plutil', '-convert', 'json', '-o', '-', str(project / 'project.pbxproj')])
    contents = json.loads(data)
    targets = [(key, value) for key, value in contents['objects'].items()
               if value.get('isa') == 'PBXNativeTarget' and value.get('name') == name]
    if len(targets) != 1:
        raise RuntimeError('Expected one independent ' + name + ' native target')
    target_id, target = targets[0]
    product_id = target['productReference']
    return target_id, product_id, contents['objects'][product_id]['path']


def watch_target(project):
    return native_target(project, 'LokiMetrisWatch')


def widget_target(project):
    return native_target(project, 'LokiMetrisWidgets')


def deployment_target(simulator, supplied):
    sdk = 'iphonesimulator' if simulator else 'iphoneos'
    path = Path(subprocess.check_output(['xcrun', '--sdk', sdk, '--show-sdk-path'], text=True).strip())
    settings = plistlib.loads((path / 'SDKSettings.plist').read_bytes())
    minimum = settings['SupportedTargets'][sdk]['MinimumDeploymentTarget']
    version = lambda value: tuple(int(part) for part in value.split('.'))
    return max([minimum, supplied], key=version)


def generate_project(directory, app, watch_project=WATCH_PROJECT, *, simulator, team,
                     widget_project=WIDGET_PROJECT, with_watch=True, with_widgets=True):
    """The iPhone target stays prelinked; Xcode builds and embeds native children."""
    directory.mkdir(parents=True, exist_ok=True)
    project = directory / 'NSHCompanion.xcodeproj'
    project.mkdir()
    info = plistlib.loads((app / 'Info.plist').read_bytes())
    info.pop('UIDeviceFamily', None)
    minimum = deployment_target(simulator, info.get('MinimumOSVersion', '15.0'))
    info['MinimumOSVersion'] = minimum
    if with_widgets:
        info.setdefault('LokiWidgetAppGroup', 'group.' + info['CFBundleIdentifier'])
    (directory / 'Info.plist').write_bytes(plistlib.dumps(info))
    objects = {}

    def add(label, kind, **fields):
        key = identifier(label)
        objects[key] = {'isa': kind, **fields}
        return key

    product = add('hostProduct', 'PBXFileReference', explicitFileType='wrapper.application',
                  includeInIndex=0, path=app.name, sourceTree='BUILT_PRODUCTS_DIR')
    products = add('hostProducts', 'PBXGroup', children=[product], name='Products', sourceTree='<group>')
    stage = add('stageNSH', 'PBXShellScriptBuildPhase', buildActionMask=2147483647, files=[],
                inputPaths=[str(app / info['CFBundleExecutable'])],
                outputPaths=['$(TARGET_BUILD_DIR)/$(EXECUTABLE_PATH)'], runOnlyForDeploymentPostprocessing=0,
                shellPath='/bin/sh', name='Stage prelinked NSH application',
                shellScript='set -eu\n/usr/bin/ditto --norsrc --noqtn "$NSH_PRELINKED_APP" "$TARGET_BUILD_DIR/$WRAPPER_NAME"\n')
    children = [products]; dependencies = []; embeds = []; references = []

    def embed_target(label, external_project, target_name, target, *, extension):
        target_id, product_id, product_name = target
        external = add(label + 'Project', 'PBXFileReference', lastKnownFileType='wrapper.pb-project',
                       path=str(external_project), sourceTree='<absolute>')
        target_proxy = add(label + 'TargetProxy', 'PBXContainerItemProxy', containerPortal=external,
                           proxyType=1, remoteGlobalIDString=target_id, remoteInfo=target_name)
        product_proxy = add(label + 'ProductProxy', 'PBXContainerItemProxy', containerPortal=external,
                            proxyType=2, remoteGlobalIDString=product_id, remoteInfo=product_name)
        reference = add(label + 'Reference', 'PBXReferenceProxy',
                        fileType='wrapper.app-extension' if extension else 'wrapper.application',
                        path=product_name, remoteRef=product_proxy, sourceTree='BUILT_PRODUCTS_DIR')
        external_products = add(label + 'Products', 'PBXGroup', children=[reference],
                                name=target_name + ' Products', sourceTree='<group>')
        dependencies.append(add(label + 'Dependency', 'PBXTargetDependency', name=target_name,
                                targetProxy=target_proxy))
        attributes = ['RemoveHeadersOnCopy'] + (['CodeSignOnCopy'] if extension else [])
        embedded = add(label + 'BuildFile', 'PBXBuildFile', fileRef=reference, settings={'ATTRIBUTES': attributes})
        embeds.append(add(label + 'Embed', 'PBXCopyFilesBuildPhase', buildActionMask=2147483647,
                          dstPath='' if extension else '$(CONTENTS_FOLDER_PATH)/Watch',
                          dstSubfolderSpec=13 if extension else 16, files=[embedded],
                          name='Embed App Extensions' if extension else 'Embed Watch Content',
                          runOnlyForDeploymentPostprocessing=0))
        children.append(external)
        references.append({'ProductGroup': external_products, 'ProjectRef': external})

    if with_watch:
        embed_target('watch', watch_project, 'LokiMetrisWatch', watch_target(watch_project), extension=False)
    if with_widgets:
        embed_target('widget', widget_project, 'LokiMetrisWidgets', widget_target(widget_project), extension=True)
    group = add('mainGroup', 'PBXGroup', children=children, sourceTree='<group>')
    settings = {
        'PRODUCT_BUNDLE_IDENTIFIER': info['CFBundleIdentifier'], 'PRODUCT_NAME': app.stem,
        'EXECUTABLE_NAME': info['CFBundleExecutable'], 'INFOPLIST_FILE': str(directory / 'Info.plist'),
        'GENERATE_INFOPLIST_FILE': 'NO', 'SDKROOT': 'iphoneos',
        'SUPPORTED_PLATFORMS': 'iphoneos iphonesimulator', 'IPHONEOS_DEPLOYMENT_TARGET': minimum,
        'TARGETED_DEVICE_FAMILY': '1,2', 'CODE_SIGN_STYLE': 'Manual', 'CODE_SIGN_IDENTITY': '-',
        'DEVELOPMENT_TEAM': team, 'SKIP_INSTALL': 'NO', 'INSTALL_PATH': '$(LOCAL_APPS_DIR)',
        'COPY_PHASE_STRIP': 'NO', 'STRIP_INSTALLED_PRODUCT': 'NO', 'ENABLE_USER_SCRIPT_SANDBOXING': 'NO',
        'NSH_PRELINKED_APP': str(app), 'ENABLE_DEBUG_DYLIB': 'NO', 'ALWAYS_SEARCH_USER_PATHS': 'NO',
    }
    configs = []; project_configs = []
    for name in ['Debug', 'Release']:
        configs.append(add('host' + name, 'XCBuildConfiguration', buildSettings=settings, name=name))
        project_configs.append(add('project' + name, 'XCBuildConfiguration', buildSettings={}, name=name))
    host_configs = add('hostConfigurations', 'XCConfigurationList', buildConfigurations=configs,
                       defaultConfigurationIsVisible=0, defaultConfigurationName='Release')
    pconfigs = add('projectConfigurations', 'XCConfigurationList', buildConfigurations=project_configs,
                   defaultConfigurationIsVisible=0, defaultConfigurationName='Release')
    host = add('host', 'PBXNativeTarget', buildConfigurationList=host_configs, buildPhases=[stage] + embeds,
               buildRules=[], dependencies=dependencies, name='NSHCompanion', productName=app.stem,
               productReference=product, productType='com.apple.product-type.application')
    root = add('project', 'PBXProject', attributes={'LastUpgradeCheck': '2710'}, buildConfigurationList=pconfigs,
               compatibilityVersion='Xcode 14.0', developmentRegion='en', hasScannedForEncodings=0,
               knownRegions=['en', 'Base'], mainGroup=group, productRefGroup=products,
               projectReferences=references, projectDirPath='', projectRoot='', targets=[host])
    (project / 'project.pbxproj').write_text('// !$*UTF8*$!\n' + project_value({
        'archiveVersion': 1, 'classes': {}, 'objectVersion': 56, 'objects': objects, 'rootObject': root}) + '\n')
    schemes = project / 'xcshareddata/xcschemes'; schemes.mkdir(parents=True)
    (schemes / 'NSHCompanion.xcscheme').write_text(f'''<?xml version="1.0" encoding="UTF-8"?>
<Scheme LastUpgradeVersion="2710" version="1.3"><BuildAction parallelizeBuildables="YES" buildImplicitDependencies="YES">
<BuildActionEntries><BuildActionEntry buildForTesting="YES" buildForRunning="YES" buildForProfiling="YES" buildForArchiving="YES" buildForAnalyzing="YES">
<BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="{host}" BuildableName="{escape(app.name, {chr(34): '&quot;'})}" BlueprintName="NSHCompanion" ReferencedContainer="container:NSHCompanion.xcodeproj"/>
</BuildActionEntry></BuildActionEntries></BuildAction><ArchiveAction buildConfiguration="Release" revealArchiveInOrganizer="NO"/></Scheme>''')
    return project


def machine_code_hash(executable):
    """Code signatures may change; the prelinked __text instructions must not."""
    with executable.open('rb') as source:
        header = source.read(32)
        if header[:4] != b'\xcf\xfa\xed\xfe':
            raise RuntimeError('Expected a thin 64-bit NSH Mach-O executable')
        for _ in range(struct.unpack_from('<I', header, 16)[0]):
            position = source.tell()
            _, size = struct.unpack('<II', source.read(8))
            source.seek(position); command = source.read(size)
            if struct.unpack_from('<I', command)[0] != 0x19:
                continue
            for index in range(struct.unpack_from('<I', command, 64)[0]):
                section = 72 + index * 80
                if command[section:section + 16].split(b'\0')[0] == b'__text':
                    length = struct.unpack_from('<Q', command, section + 40)[0]
                    offset = struct.unpack_from('<I', command, section + 48)[0]
                    source.seek(offset)
                    return hashlib.sha256(source.read(length)).hexdigest()
    raise RuntimeError('NSH executable has no __text section')



def macho_platforms(executable):
    """Inspect LC_BUILD_VERSION in every slice; ARM64 alone is ambiguous."""
    with executable.open('rb') as source:
        magic = source.read(4)
        fat = {b'\xca\xfe\xba\xbe': ('>', False), b'\xca\xfe\xba\xbf': ('>', True),
               b'\xbe\xba\xfe\xca': ('<', False), b'\xbf\xba\xfe\xca': ('<', True)}
        if magic in fat:
            endian, wide = fat[magic]
            count = struct.unpack(endian + 'I', source.read(4))[0]
            bases = []
            for _ in range(count):
                entry = source.read(32 if wide else 20)
                bases.append(struct.unpack_from(endian + ('Q' if wide else 'I'), entry, 8)[0])
        else:
            bases = [0]
        result = set()
        formats = {b'\xcf\xfa\xed\xfe': ('<', 32), b'\xce\xfa\xed\xfe': ('<', 28),
                   b'\xfe\xed\xfa\xcf': ('>', 32), b'\xfe\xed\xfa\xce': ('>', 28)}
        for base in bases:
            source.seek(base); header = source.read(32)
            if header[:4] not in formats:
                raise RuntimeError('Unsupported Mach-O header')
            endian, header_size = formats[header[:4]]
            count = struct.unpack_from(endian + 'I', header, 16)[0]
            source.seek(base + header_size)
            platform = None
            for _ in range(count):
                position = source.tell()
                kind, size = struct.unpack(endian + 'II', source.read(8))
                if kind == 0x32:
                    platform = struct.unpack(endian + 'I', source.read(4))[0]
                    break
                source.seek(position + size)
            if platform is None:
                raise RuntimeError('Mach-O lacks an explicit LC_BUILD_VERSION platform')
            result.add(platform)
        return result


def archived_application(archive):
    info = plistlib.loads((archive / 'Info.plist').read_bytes())
    relative = Path(info['ApplicationProperties']['ApplicationPath'])
    if relative.is_absolute() or '..' in relative.parts:
        raise RuntimeError('Invalid application path in Xcode archive metadata')
    return archive / 'Products' / relative


def nsh_bundle_hash(app):
    bundles = list(app.rglob('bundle.nshc'))
    if len(bundles) != 1:
        raise RuntimeError('Expected one NSH application bundle in the iPhone app')
    return hashlib.sha256(bundles[0].read_bytes()).hexdigest()


def watch_source_hashes(project):
    """Bind the archive to a stable source snapshot, excluding Xcode user state."""
    root = project.parent
    ignored = {'xcuserdata', 'project.xcworkspace', '.DS_Store', '__pycache__', '.build', 'build', 'target', '.git'}
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(root.rglob('*'))
            if path.is_file() and not ignored.intersection(path.relative_to(root).parts)}


def prepare_app(app, prepared, companion_id):
    shutil.copytree(app, prepared, symlinks=True)
    # Reassembly may reuse a previously combined phone. Remove its old child by
    # companion metadata so only the current Xcode-built Watch product is staged.
    for child in list(prepared.rglob('*.app')):
        metadata = plistlib.loads((child / 'Info.plist').read_bytes())
        if metadata.get('WKCompanionAppBundleIdentifier') == companion_id:
            shutil.rmtree(child)
    for child in list(prepared.rglob('*.appex')):
        metadata = plistlib.loads((child / 'Info.plist').read_bytes())
        if metadata.get('CFBundleIdentifier') == companion_id + '.widgets':
            shutil.rmtree(child)


def assemble(app, archive, *, simulator=True, log=None, watch_project=WATCH_PROJECT,
             widget_project=WIDGET_PROJECT, widget_library=None, with_watch=True, with_widgets=True):
    app = app.resolve(strict=True); archive = archive.resolve()
    if archive.exists():
        raise RuntimeError('Refusing to replace an existing companion archive')
    info = plistlib.loads((app / 'Info.plist').read_bytes())
    phone_platforms = macho_platforms(app / info['CFBundleExecutable'])
    if phone_platforms != {7 if simulator else 2}:
        raise RuntimeError('The prelinked NSH executable does not match the requested iOS platform')
    bundle_hash = nsh_bundle_hash(app)
    source_hashes = watch_source_hashes(watch_project) if with_watch else None
    widget_hashes = watch_source_hashes(widget_project) if with_widgets else None
    if with_widgets and (widget_library is None or not Path(widget_library).is_file()):
        raise RuntimeError('Widget assembly requires the matching Rust static library')
    identity = json.loads((ROOT / 'signing/apple/identity.json').read_text())
    build_root = ROOT / 'output/build/ios-companion'
    build_root.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix='assembly-', dir=build_root))
    prepared = work / 'Prelinked' / app.name
    prepare_app(app, prepared, info['CFBundleIdentifier'])
    project = generate_project(work, prepared, watch_project.resolve(), simulator=simulator,
                               team=identity.get('teamId', ''), widget_project=widget_project.resolve(),
                               with_watch=with_watch, with_widgets=with_widgets)
    destination = 'generic/platform=iOS Simulator' if simulator else 'generic/platform=iOS'
    command = ['xcodebuild', '-project', project, '-scheme', 'NSHCompanion', '-configuration', 'Release',
               '-destination', destination, '-archivePath', archive, '-derivedDataPath', work / 'DerivedData',
               'LOKI_COMPANION_BUNDLE_IDENTIFIER=' + info['CFBundleIdentifier'],
               'CURRENT_PROJECT_VERSION=' + str(info['CFBundleVersion']),
               'MARKETING_VERSION=' + info['CFBundleShortVersionString'],
               'DEVELOPMENT_TEAM=' + identity.get('teamId', ''), 'archive']
    if with_widgets:
        command[-1:-1] = ['LOKI_WIDGET_CORE_LIBRARY=' + str(Path(widget_library).resolve()),
                          'LOKI_WIDGET_APP_GROUP=' + info['LokiWidgetAppGroup']]
    if not simulator:
        command.insert(-1, 'CODE_SIGNING_ALLOWED=NO')
    else:
        command[-1:-1] = ['ARCHS=arm64', 'ONLY_ACTIVE_ARCH=YES']
    run(command, log=log)
    if with_watch and watch_source_hashes(watch_project) != source_hashes:
        raise RuntimeError('Watch sources changed during compilation; rebuild after they are frozen')
    if with_widgets and watch_source_hashes(widget_project) != widget_hashes:
        raise RuntimeError('Widget sources changed during compilation; rebuild after they are frozen')
    result = archived_application(archive)
    result_info = plistlib.loads((result / 'Info.plist').read_bytes())
    if result_info['CFBundleIdentifier'] != info['CFBundleIdentifier']:
        raise RuntimeError('Xcode assembly changed the NSH application identifier')
    before = machine_code_hash(app / info['CFBundleExecutable'])
    after = machine_code_hash(result / result_info['CFBundleExecutable'])
    if before != after:
        raise RuntimeError('The archive did not preserve the prelinked NSH machine code')
    if nsh_bundle_hash(result) != bundle_hash:
        raise RuntimeError('The archive did not preserve the NSH application bundle')
    watches = []
    for candidate in result.rglob('*.app'):
        metadata = plistlib.loads((candidate / 'Info.plist').read_bytes())
        if metadata.get('WKCompanionAppBundleIdentifier') == info['CFBundleIdentifier']:
            watches.append((candidate, metadata))
    if len(watches) != (1 if with_watch else 0):
        raise RuntimeError('Embedded Watch application count differs from the requested build')
    report = {'archive': archive.name, 'applicationPath': str(result.relative_to(archive)),
              'companionBundleId': info['CFBundleIdentifier'], 'machineCodePreserved': True,
              'machineCodeSha256': after, 'nshBundlePreserved': True, 'nshBundleSha256': bundle_hash,
              'phoneBuildPlatforms': sorted(phone_platforms),
              'simulatorOnly': simulator, 'signatureVerified': simulator,
              'deviceSigningRequired': not simulator, 'runtimeVerified': False}
    if watches:
        watch, watch_info = watches[0]
        watch_platforms = macho_platforms(watch / watch_info['CFBundleExecutable'])
        if watch_platforms != {9 if simulator else 4}:
            raise RuntimeError('The embedded Watch executable does not match the requested watchOS platform')
        verify_versions(info, watch_info)
        report.update({'watchPath': str(watch.relative_to(result)), 'watchBundleId': watch_info['CFBundleIdentifier'],
                       'watchSourceHashes': source_hashes,
                       'watchExecutableSha256': hashlib.sha256((watch / watch_info['CFBundleExecutable']).read_bytes()).hexdigest(),
                       'watchBuildPlatforms': sorted(watch_platforms)})
    extensions = list(result.rglob('*.appex'))
    if len(extensions) != (1 if with_widgets else 0):
        raise RuntimeError('Embedded WidgetKit extension count differs from the requested build')
    if extensions:
        widget = extensions[0]
        widget_info = plistlib.loads((widget / 'Info.plist').read_bytes())
        expected_group = info['LokiWidgetAppGroup']
        if widget.parent != result / 'PlugIns' or widget_info.get('CFBundleIdentifier') != info['CFBundleIdentifier'] + '.widgets':
            raise RuntimeError('The WidgetKit extension has an unexpected identifier or embedding path')
        if widget_info.get('NSExtension', {}).get('NSExtensionPointIdentifier') != 'com.apple.widgetkit-extension':
            raise RuntimeError('The embedded extension is not a WidgetKit extension')
        if result_info.get('LokiWidgetAppGroup') != expected_group or widget_info.get('LokiWidgetAppGroup') != expected_group:
            raise RuntimeError('Phone and Widget App Group declarations differ')
        widget_platforms = macho_platforms(widget / widget_info['CFBundleExecutable'])
        if widget_platforms != {7 if simulator else 2}:
            raise RuntimeError('The embedded Widget executable does not match the requested iOS platform')
        verify_versions(info, widget_info)
        report.update({'widgetPath': str(widget.relative_to(result)), 'widgetBundleId': widget_info['CFBundleIdentifier'],
                       'widgetAppGroup': expected_group, 'widgetSourceHashes': widget_hashes,
                       'widgetExecutableSha256': hashlib.sha256((widget / widget_info['CFBundleExecutable']).read_bytes()).hexdigest(),
                       'widgetCoreSha256': hashlib.sha256(Path(widget_library).read_bytes()).hexdigest(),
                       'widgetBuildPlatforms': sorted(widget_platforms)})
    if simulator:
        run(['codesign', '--verify', '--deep', '--strict', result], log=log)
    (archive.parent / 'companion-archive.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    return result, report


def verify_versions(phone, child):
    if str(child['CFBundleVersion']) != str(phone['CFBundleVersion']):
        raise RuntimeError('Phone and embedded application build versions differ')
    if child['CFBundleShortVersionString'] != phone['CFBundleShortVersionString']:
        raise RuntimeError('Phone and embedded application marketing versions differ')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', required=True, type=Path)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--device', action='store_true', help='Archive unsigned device binaries; no device installation or distribution')
    parser.add_argument('--without-watch', action='store_true', help='Embed Widgets without a Watch companion')
    parser.add_argument('--widget-core-library', required=True, type=Path)
    args = parser.parse_args()
    app, _ = assemble(args.app, args.archive, simulator=not args.device,
                      with_watch=not args.without_watch, widget_library=args.widget_core_library)
    print(app)


if __name__ == '__main__':
    main()
