#!/usr/bin/env python3
"""COSMIC Alt+Tab MRU 1.4.0 — instalace mistnich zdroju pouze na Linuxu.

Spusteni instalace ze slozky projektu (BEZ sudo):
  python3 install.py

Pouze testy, sestaveni a kontrola Waylandu, bez vymeny instalace:
  python3 install.py --check-only

Obnoveni predchozi zalohy:
  python3 install.py --rollback
"""
import argparse
import datetime
from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time

VERSION = '1.4.0'

SERVICE = 'cosmic-ext-app-switcher-mru.service'
BASE = Path.home() / '.local/share/cosmic-alt-tab-mru'
BIN = Path.home() / '.local/bin/cosmic-ext-app-switcher'
CONFIG = Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config')))
UNIT = CONFIG / 'systemd/user' / SERVICE
START = Path.home() / '.local/bin/cosmic-ext-app-switcher-mru-start'
AUTOSTART = CONFIG / 'autostart/cosmic-ext-app-switcher-mru.desktop'
PROJECT_DIR = Path(__file__).resolve().parent


def run(argv, **kwargs):
    return subprocess.run([str(x) for x in argv], check=True, **kwargs)


def require_linux():
    if sys.platform != 'linux':
        raise RuntimeError('Instalator je pouze pro Linux. Na macOS se zadne soubory instalace nemeni.')
    if os.geteuid() == 0:
        raise RuntimeError('Spust instalator jako svuj bezny uzivatel, BEZ sudo.')



@contextmanager
def idle_ui():
    runtime = os.environ.get('XDG_RUNTIME_DIR')
    if not runtime:
        raise RuntimeError('Chybi XDG_RUNTIME_DIR; spust v prihlasene relaci COSMIC.')
    with (Path(runtime) / 'cosmic-ext-app-switcher-ui.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('Prepinac je jeste otevreny nebo se zavira. Zavri Alt+Tab a opakuj instalaci.') from None
        # Older versions do not keep the lifetime lock. Refuse to replace a live UI.
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(0.5)
            try:
                client.connect(str(Path(runtime) / 'cosmic-ext-app-switcher.sock'))
            except (FileNotFoundError, ConnectionRefusedError):
                pass
            else:
                raise RuntimeError('Bezi predchozi UI prepinace. Zavri Alt+Tab pred instalaci.')
        yield


def status():
    return subprocess.run(['systemctl', '--user', 'is-active', '--quiet', SERVICE]).returncode == 0


def atomic_bytes(path, data, mode):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix='.' + path.name + '.', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(tmp, mode)
        os.replace(tmp, path)
    finally:
        if os.path.lexists(tmp):
            os.unlink(tmp)


def backup(paths):
    folder = BASE / 'backups' / datetime.datetime.now().strftime('%Y%m%d-%H%M%S-%f')
    folder.mkdir(parents=True, mode=0o700)
    items = []
    for i, target in enumerate(paths):
        item = {'path': str(target), 'kind': 'missing'}
        if target.is_symlink():
            item.update(kind='symlink', link=os.readlink(target))
        elif target.is_file():
            name = str(i) + '.backup'
            shutil.copy2(target, folder / name)
            item.update(kind='file', file=name, mode=target.stat().st_mode & 0o777)
        elif target.exists():
            raise RuntimeError(f'Neocekavany typ cile: {target}')
        items.append(item)
    manifest = {'items': items, 'service_was_active': status()}
    (folder / 'manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
    return folder


def restore(folder):
    manifest = json.loads((folder / 'manifest.json').read_text(encoding='utf-8'))
    expected = {str(x) for x in [BIN, UNIT, START, AUTOSTART]}
    if {item['path'] for item in manifest['items']} != expected:
        raise RuntimeError('Zaloha neodpovida teto instalaci.')
    for item in manifest['items']:
        if item['kind'] == 'file' and not (folder / item['file']).is_file():
            raise RuntimeError('V zaloze chybi soubor; obnova nebyla zahajena.')
    subprocess.run(['systemctl', '--user', 'stop', SERVICE], check=False)
    for item in manifest['items']:
        target = Path(item['path'])
        if item['kind'] == 'file':
            atomic_bytes(target, (folder / item['file']).read_bytes(), item['mode'])
        elif item['kind'] == 'symlink':
            target.unlink(missing_ok=True)
            target.symlink_to(item['link'])
        else:
            target.unlink(missing_ok=True)
    run(['systemctl', '--user', 'daemon-reload'])
    if manifest['service_was_active']:
        run(['systemctl', '--user', 'start', SERVICE])
    print('Predchozi instalace byla obnovena.', flush=True)


def import_environment():
    names = [name for name in ['WAYLAND_DISPLAY', 'DISPLAY', 'XDG_CURRENT_DESKTOP', 'XDG_SESSION_TYPE'] if name in os.environ]
    if names:
        run(['systemctl', '--user', 'import-environment', *names])


def desktop_quote(value):
    value = value.replace('%', '%%')
    for character in ['\\', '"', '`', '$']:
        value = value.replace(character, '\\' + character)
    value = value.replace('\\', '\\\\')
    return '"' + value + '"'


def install(source_dir: Path, check_only=False):
    require_linux()
    if not os.environ.get('WAYLAND_DISPLAY') or not os.environ.get('XDG_RUNTIME_DIR'):
        raise RuntimeError('Spust instalator v terminalu uvnitr prihlaseneho COSMIC.')
    for relative in ['Cargo.toml', 'Cargo.lock', 'src/main.rs', 'src/ipc.rs', 'config/Cargo.toml', 'applet/Cargo.toml']:
        if not (source_dir / relative).is_file():
            raise RuntimeError(f'Chybi {relative}. Na Linux prenes celou slozku ver 1.4.')

    cargo = shutil.which('cargo')
    if not cargo and (Path.home() / '.cargo/bin/cargo').is_file():
        cargo = str(Path.home() / '.cargo/bin/cargo')
    if not cargo:
        raise RuntimeError('Chybi cargo/Rust.')
    if not shutil.which('systemctl'):
        raise RuntimeError('Chybi systemctl pro uzivatelskou sluzbu.')
    pkgconfig = shutil.which('pkg-config')
    if not pkgconfig or subprocess.run([pkgconfig, '--exists', 'wayland-client', 'xkbcommon']).returncode:
        raise RuntimeError('Chybi pkg-config nebo vyvojove knihovny wayland-client/xkbcommon.')
    run(['systemctl', '--user', 'show-environment'], stdout=subprocess.DEVNULL)

    env = os.environ.copy()
    env['PATH'] = str(Path(cargo).parent) + os.pathsep + env.get('PATH', '')
    env['CARGO_TARGET_DIR'] = str(Path.home() / '.cache/cosmic-alt-tab-mru/target')
    env.setdefault('CARGO_BUILD_JOBS', '2')

    print(f'1/3 Testuji a kompiluji verzi {VERSION} ze slozky: {source_dir}', flush=True)
    run([cargo, 'test', '--locked', '--bin', 'cosmic-ext-app-switcher'], cwd=source_dir, env=env)
    run([cargo, 'build', '--locked', '--release', '--bin', 'cosmic-ext-app-switcher'], cwd=source_dir, env=env)
    new_binary = Path(env['CARGO_TARGET_DIR']) / 'release/cosmic-ext-app-switcher'
    version = run([new_binary, '--version'], capture_output=True, text=True).stdout.strip()
    if version != f'cosmic-ext-app-switcher {VERSION}':
        raise RuntimeError(f'Sestavena neocekavana verze: {version}')
    run([new_binary, '--check-compat'])
    if check_only:
        print('Kontrola hotova. Testy, sestaveni a Wayland kontrola prosly. Instalace zustava beze zmen.')
        return

    with idle_ui():
        install_binary(new_binary)


def install_binary(new_binary):
    print('2/3 Vytvarim zalohu a instaluji prepinac.', flush=True)
    folder = backup([BIN, UNIT, START, AUTOSTART])
    try:
        subprocess.run(['systemctl', '--user', 'stop', SERVICE], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        atomic_bytes(BIN, new_binary.read_bytes(), 0o755)
        unit = '''[Unit]
Description=COSMIC Alt-Tab per-window MRU history
PartOf=graphical-session.target
After=graphical-session.target
StartLimitIntervalSec=60
StartLimitBurst=3

[Service]
Type=simple
ExecStart="%h/.local/bin/cosmic-ext-app-switcher" --daemon
Restart=on-failure
RestartSec=5
UMask=0077
'''
        atomic_bytes(UNIT, unit.encode(), 0o644)
        starter = '''#!/bin/sh
set -eu
[ -n "${WAYLAND_DISPLAY:-}" ] || exit 0
systemctl --user import-environment WAYLAND_DISPLAY
[ -z "${DISPLAY:-}" ] || systemctl --user import-environment DISPLAY
systemctl --user restart cosmic-ext-app-switcher-mru.service
'''
        atomic_bytes(START, starter.encode(), 0o755)
        desktop = '[Desktop Entry]\nType=Application\nName=COSMIC Alt-Tab MRU history\nExec=' + desktop_quote(str(START)) + '\nTerminal=false\nOnlyShowIn=COSMIC;\n'
        atomic_bytes(AUTOSTART, desktop.encode(), 0o644)
        run(['systemctl', '--user', 'daemon-reload'])
        import_environment()
        run(['systemctl', '--user', 'start', SERVICE])

        print('3/3 Kontroluji spojeni s MRU sluzbou.', flush=True)
        deadline = time.monotonic() + 15
        while True:
            result = subprocess.run([BIN, '--mru-check'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if result.returncode == 0:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError('Sluzba se nespustila; obnovuji puvodni instalaci.')
            time.sleep(0.2)
        atomic_bytes(BASE / 'last-backup', (str(folder) + '\n').encode(), 0o600)
    except BaseException:
        restore(folder)
        raise

    print('\nHOTOVO. Prepinac je nainstalovany a sluzba bezi.')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    group = parser.add_mutually_exclusive_group()
    group.add_argument('--rollback', action='store_true', help='Obnovi predchozi zalohu')
    group.add_argument('--check-only', action='store_true', help='Testy a sestaveni bez vymeny instalace (pouze Linux/COSMIC)')
    args = parser.parse_args()

    require_linux()
    if args.check_only:
        install(PROJECT_DIR, check_only=True)
        return
    BASE.mkdir(parents=True, exist_ok=True)
    with (BASE / 'installer.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args.rollback:
            marker = BASE / 'last-backup'
            if not marker.is_file():
                raise RuntimeError('Nebyla nalezena zaloha z uspesne instalace.')
            with idle_ui():
                restore(Path(marker.read_text().strip()))
                marker.unlink()
        else:
            install(PROJECT_DIR)


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print('\nChyba:', error, file=sys.stderr)
        raise SystemExit(1)
