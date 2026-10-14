#!/usr/bin/env python3
"""COSMIC Alt+Tab MRU reusable source installer (Anti-Flicker Edition).

Spusteni instalace ze slozky projektu (BEZ sudo):
  python3 install.py

Obnoveni predchozi zalohy:
  python3 install.py --rollback
"""
import argparse
import datetime
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

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


def install(source_dir: Path):
    if os.geteuid() == 0:
        raise RuntimeError('Spust instalator jako svuj bezny uzivatel, BEZ sudo.')
    if not os.environ.get('WAYLAND_DISPLAY') or not os.environ.get('XDG_RUNTIME_DIR'):
        raise RuntimeError('Spust instalator v terminalu uvnitr prihlaseneho COSMIC.')
    if not (source_dir / 'Cargo.toml').is_file():
        raise RuntimeError(f'Ve slozce {source_dir} chybi Cargo.toml.')

    cargo = shutil.which('cargo')
    if not cargo and (Path.home() / '.cargo/bin/cargo').is_file():
        cargo = str(Path.home() / '.cargo/bin/cargo')
    if not cargo:
        raise RuntimeError('Chybi cargo/Rust.')

    env = os.environ.copy()
    env['PATH'] = str(Path(cargo).parent) + os.pathsep + env.get('PATH', '')
    env['CARGO_TARGET_DIR'] = str(Path.home() / '.cache/cosmic-alt-tab-mru/target')
    env.setdefault('CARGO_BUILD_JOBS', '4')

    print(f'1/3 Kompiluji ze slozky: {source_dir}', flush=True)
    run([cargo, 'build', '--locked', '--release', '--bin', 'cosmic-ext-app-switcher'], cwd=source_dir, env=env)
    new_binary = Path(env['CARGO_TARGET_DIR']) / 'release/cosmic-ext-app-switcher'
    run([new_binary, '--check-compat'])

    print('2/3 Vytvarim zalohu a instaluji prepinac.', flush=True)
    folder = backup([BIN, UNIT, START, AUTOSTART])
    try:
        subprocess.run(['systemctl', '--user', 'stop', SERVICE], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        atomic_bytes(BIN, new_binary.read_bytes(), 0o755)
        unit = '''[Unit]
Description=COSMIC Alt-Tab per-window MRU history
PartOf=graphical-session.target
After=graphical-session.target

[Service]
Type=simple
ExecStart="%h/.local/bin/cosmic-ext-app-switcher" --daemon
Restart=on-failure
RestartSec=2
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
    parser.add_argument('--rollback', action='store_true', help='Obnovi predchozi zalohu')
    parser.add_argument('--source', type=Path, default=PROJECT_DIR, help='Cesta ke slozce s Cargo.toml')
    args = parser.parse_args()

    BASE.mkdir(parents=True, exist_ok=True)
    with (BASE / 'installer.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args.rollback:
            marker = BASE / 'last-backup'
            if not marker.is_file():
                raise RuntimeError('Nebyla nalezena zaloha z uspesne instalace.')
            restore(Path(marker.read_text().strip()))
            marker.unlink()
        else:
            install(args.source.resolve())


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print('\nChyba:', error, file=sys.stderr)
        raise SystemExit(1)