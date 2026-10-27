#!/usr/bin/env python3
"""Sign an immutable NSH device build for an existing development identity; never install it."""
import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import plistlib
import re
import shutil
import struct
import subprocess

from package_apple_companion import macho_platforms, nsh_bundle_hash


def command(arguments):
    return subprocess.run([str(item) for item in arguments], check=True, capture_output=True,
                          timeout=90).stdout


def profile(path):
    return plistlib.loads(command(['security', 'cms', '-D', '-i', path]))


def permits(pattern, identifier):
    if '*' not in pattern:
        return pattern == identifier
    return pattern.endswith('.*') and pattern.count('*') == 1 and identifier.startswith(pattern[:-1])


def certificate(fingerprint):
    identities = command(['security', 'find-identity', '-v', '-p', 'codesigning']).decode()
    if fingerprint not in identities:
        raise RuntimeError('The selected certificate has no valid local code-signing identity')
    certificates = command(['security', 'find-certificate', '-a', '-c', 'Apple Development', '-p']).decode()
    for pem in re.findall(r'-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----', certificates, re.S):
        der = base64.b64decode(''.join(pem.splitlines()[1:-1]))
        if hashlib.sha1(der).hexdigest().upper() != fingerprint:
            continue
        result = subprocess.run(['openssl', 'x509', '-noout', '-subject', '-nameopt', 'sep_multiline'],
                                input=pem.encode(), check=True, capture_output=True)
        team = re.search(r'^\s*OU\s*=\s*([A-Z0-9]{10})\s*$', result.stdout.decode(), re.M)
        if team:
            return team.group(1)
    raise RuntimeError('Cannot establish the Apple Development certificate team from its public certificate')


def development_entitlements(value, bundle_id, team, fingerprint, *, device=None):
    """Validate the development grant and request only the default private Keychain group."""
    expires = value.get('ExpirationDate')
    if not isinstance(expires, datetime):
        raise RuntimeError('Provisioning profile has no expiry')
    if expires.replace(tzinfo=timezone.utc) <= datetime.now(timezone.utc):
        raise RuntimeError('Provisioning profile has expired')
    if value.get('TeamIdentifier') != [team]:
        raise RuntimeError('Provisioning profile belongs to another development team')
    entitlements = value.get('Entitlements', {})
    if entitlements.get('get-task-allow') is not True or not value.get('ProvisionedDevices'):
        raise RuntimeError('Expected a development profile for registered devices')
    if device and device not in value['ProvisionedDevices']:
        raise RuntimeError('The target device is not included in the provisioning profile')
    certificates = {hashlib.sha1(item).hexdigest().upper() for item in value.get('DeveloperCertificates', [])}
    if fingerprint not in certificates:
        raise RuntimeError('Provisioning profile does not include the selected signing certificate')
    grant = entitlements.get('application-identifier', '')
    prefix = grant.split('.', 1)[0]
    requested_id = prefix + '.' + bundle_id
    if prefix not in value.get('ApplicationIdentifierPrefix', []) or not permits(grant, requested_id):
        raise RuntimeError('Provisioning profile does not permit the requested application identifier')
    if not any(permits(pattern, requested_id) for pattern in entitlements.get('keychain-access-groups', [])):
        raise RuntimeError('Provisioning profile does not permit the private Keychain access group')
    return {'application-identifier': requested_id, 'com.apple.developer.team-identifier': team,
            'get-task-allow': True, 'keychain-access-groups': [requested_id]}


def executable(app, info):
    name = info['CFBundleExecutable']
    if Path(name).name != name or name in ['', '.', '..']:
        raise RuntimeError('Invalid bundle executable path')
    result = app / name
    if not result.resolve().is_relative_to(app.resolve()):
        raise RuntimeError('Bundle executable escapes its application')
    return result


def instructions(path):
    """Hash __TEXT,__text in every Mach-O slice, independent of signing blobs."""
    data = path.read_bytes()
    fat = {b'\xca\xfe\xba\xbe': ('>', False), b'\xca\xfe\xba\xbf': ('>', True),
           b'\xbe\xba\xfe\xca': ('<', False), b'\xbf\xba\xfe\xca': ('<', True)}
    if data[:4] in fat:
        endian, wide = fat[data[:4]]
        count = struct.unpack_from(endian + 'I', data, 4)[0]
        bases = [struct.unpack_from(endian + ('Q' if wide else 'I'), data,
                                    8 + index * (32 if wide else 20) + 8)[0] for index in range(count)]
    else:
        bases = [0]
    formats = {b'\xcf\xfa\xed\xfe': ('<', 32), b'\xce\xfa\xed\xfe': ('<', 28),
               b'\xfe\xed\xfa\xcf': ('>', 32), b'\xfe\xed\xfa\xce': ('>', 28)}
    result = {}
    for base in bases:
        endian, header_size = formats[data[base:base + 4]]
        cpu, subtype = struct.unpack_from(endian + 'II', data, base + 4)
        count = struct.unpack_from(endian + 'I', data, base + 16)[0]
        position = base + header_size
        found = False
        for _ in range(count):
            kind, size = struct.unpack_from(endian + 'II', data, position)
            if kind in [1, 0x19]:
                wide = kind == 0x19
                section_count = struct.unpack_from(endian + 'I', data, position + (64 if wide else 48))[0]
                for index in range(section_count):
                    section = position + (72 if wide else 56) + index * (80 if wide else 68)
                    if data[section:section + 16].split(b'\0')[0] != b'__text':
                        continue
                    if data[section + 16:section + 32].split(b'\0')[0] != b'__TEXT':
                        continue
                    length = struct.unpack_from(endian + ('Q' if wide else 'I'), data, section + (40 if wide else 36))[0]
                    offset = struct.unpack_from(endian + 'I', data, section + (48 if wide else 40))[0]
                    body = data[base + offset:base + offset + length]
                    if len(body) != length:
                        raise RuntimeError('Truncated Mach-O text section')
                    result[f'{cpu:08x}.{subtype:08x}'] = hashlib.sha256(body).hexdigest()
                    found = True
            position += size
        if not found:
            raise RuntimeError('Mach-O slice has no __TEXT,__text')
    return result


def profile_summary(value, entitlements, device):
    return {'uuid': value['UUID'], 'name': value['Name'], 'team': value['TeamIdentifier'],
            'expires': value['ExpirationDate'].isoformat(), 'entitlements': entitlements,
            'provisionedDeviceCount': len(value.get('ProvisionedDevices', [])),
            'specifiedDeviceCovered': device in value.get('ProvisionedDevices', []) if device else None}


def plan(app, bundle_id, team, fingerprint, phone_profile, watch_profile, phone_device, watch_device=None):
    app = app.resolve(strict=True)
    if certificate(fingerprint) != team:
        raise RuntimeError('Selected development certificate belongs to another team')
    info = plistlib.loads((app / 'Info.plist').read_bytes())
    if macho_platforms(executable(app, info)) != {2}:
        raise RuntimeError('Signing requires an iOS device executable, not a simulator')
    children = [(path, plistlib.loads((path / 'Info.plist').read_bytes())) for path in app.rglob('*.app')]
    if len(children) != 1 or children[0][1].get('WKCompanionAppBundleIdentifier') != info['CFBundleIdentifier']:
        raise RuntimeError('Expected exactly one matching embedded Watch application')
    if list(app.rglob('*.appex')) or list(app.rglob('*.framework')) or list(app.rglob('*.dylib')):
        raise RuntimeError('This signer requires the reviewed static two-application NSH archive')
    watch, watch_info = children[0]
    if macho_platforms(executable(watch, watch_info)) != {4}:
        raise RuntimeError('Embedded Watch executable is not a watchOS device build')
    if watch_info['CFBundleVersion'] != info['CFBundleVersion'] or watch_info['CFBundleShortVersionString'] != info['CFBundleShortVersionString']:
        raise RuntimeError('Phone and Watch versions must match')
    phone, child = profile(phone_profile), profile(watch_profile)
    watch_id = bundle_id + '.watchkitapp'
    phone_entitlements = development_entitlements(phone, bundle_id, team, fingerprint, device=phone_device)
    watch_entitlements = development_entitlements(child, watch_id, team, fingerprint, device=watch_device)
    return {'sourceApplication': str(app), 'sourceBundleId': info['CFBundleIdentifier'],
            'bundleId': bundle_id, 'watchBundleId': watch_id, 'team': team, 'certificateSha1': fingerprint,
            'phoneDeviceUdid': phone_device, 'watchDeviceUdid': watch_device,
            'phoneProfile': profile_summary(phone, phone_entitlements, phone_device),
            'watchProfile': profile_summary(child, watch_entitlements, watch_device),
            'watchRelativePath': str(watch.relative_to(app)),
            'phoneInstructions': instructions(executable(app, info)),
            'watchInstructions': instructions(executable(watch, watch_info)),
            'nshBundleSha256': nsh_bundle_hash(app), 'installsApplication': False}


def validate_output(app, output):
    if output.exists() or output.is_symlink() or output.resolve().is_relative_to(app.resolve()):
        raise RuntimeError('Output must be a new application outside the source bundle')


def sign_copy(app, output, report, phone_profile, watch_profile):
    validate_output(app, output)
    output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(app, output, symlinks=True)
    watch = output / report['watchRelativePath']
    for target, bundle_id, provision, entitlements in [
            (watch, report['watchBundleId'], watch_profile, report['watchProfile']['entitlements']),
            (output, report['bundleId'], phone_profile, report['phoneProfile']['entitlements'])]:
        info = plistlib.loads((target / 'Info.plist').read_bytes())
        info['CFBundleIdentifier'] = bundle_id
        if target == watch:
            info['WKCompanionAppBundleIdentifier'] = report['bundleId']
        (target / 'Info.plist').write_bytes(plistlib.dumps(info))
        shutil.copy2(provision, target / 'embedded.mobileprovision')
        name = 'watch' if target == watch else 'phone'
        entitlement_file = output.parent / f'{name}-entitlements.plist'
        entitlement_file.write_bytes(plistlib.dumps(entitlements))
        command(['codesign', '--force', '--sign', report['certificateSha1'], '--identifier', bundle_id,
                 '--entitlements', entitlement_file, '--generate-entitlement-der', '--timestamp=none', target])
        signed = plistlib.loads(command(['codesign', '--display', '--entitlements', ':-', target]))
        if signed != entitlements:
            raise RuntimeError('Signed entitlements differ from the reviewed development request')
    command(['codesign', '--verify', '--deep', '--strict', output])
    phone_info = plistlib.loads((output / 'Info.plist').read_bytes())
    watch_info = plistlib.loads((watch / 'Info.plist').read_bytes())
    if instructions(executable(output, phone_info)) != report['phoneInstructions']:
        raise RuntimeError('Signing changed the prelinked phone instructions')
    if instructions(executable(watch, watch_info)) != report['watchInstructions']:
        raise RuntimeError('Signing changed the prelinked Watch instructions')
    if nsh_bundle_hash(output) != report['nshBundleSha256']:
        raise RuntimeError('Signing changed the NSH bundle')
    report.update({'signedApplication': str(output.resolve()), 'signatureVerified': True,
                   'phoneInstructionsPreserved': True, 'watchInstructionsPreserved': True,
                   'nshBundlePreserved': True})
    (output.parent / 'signed-application.json').write_text(json.dumps(report, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--bundle-id', required=True)
    parser.add_argument('--team', required=True)
    parser.add_argument('--identity', required=True, help='Exact SHA-1 of the existing development signing identity')
    parser.add_argument('--phone-profile', required=True, type=Path)
    parser.add_argument('--watch-profile', required=True, type=Path)
    parser.add_argument('--device', required=True, help='Phone UDID already included in the development profile')
    parser.add_argument('--watch-device', help='Optional known Watch UDID; its absence does not block iPhone signing')
    parser.add_argument('--execute', action='store_true', help='Create and development-sign a copy; still never installs')
    args = parser.parse_args()
    fingerprint = args.identity.replace(':', '').upper()
    if not re.fullmatch(r'[A-F0-9]{40}', fingerprint) or not re.fullmatch(r'[A-Z0-9]{10}', args.team):
        parser.error('Specify an exact certificate SHA-1 and ten-character development team')
    if not re.fullmatch(r'[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+', args.bundle_id) or args.output.suffix != '.app':
        parser.error('Specify a bundle identifier and a new .app output path')
    validate_output(args.app, args.output)
    report = plan(args.app, args.bundle_id, args.team, fingerprint, args.phone_profile,
                  args.watch_profile, args.device, args.watch_device)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    (args.output.parent / 'signing-plan.json').write_text(json.dumps(report, indent=2) + '\n')
    if args.execute:
        sign_copy(args.app.resolve(), args.output, report, args.phone_profile, args.watch_profile)
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
