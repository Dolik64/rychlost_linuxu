#!/usr/bin/env python3
"""Čitelná kopie Rustových zdrojů z Python instalátorů; nic neinstaluje.

Spuštění na macOS i Linuxu:
  python3 "alt tab/tools/rozbalit_payloady.py"
  python3 "alt tab/tools/rozbalit_payloady.py" --check

Instalátory pouze parsuje pomocí AST, neimportuje je ani nespouští.
Existující odlišný soubor nepřepíše. Kopie nejsou automaticky baleny zpět.
"""

import argparse
import ast
import base64
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tarfile


ROOT = Path(__file__).resolve().parents[1]
SOURCES = {
    "ver-1.1-install": "ver 1.1/cosmic_alt_tab_mru_install.py",
    "ver-1.1-fixed": "ver 1.1/cosmic_alt_tab_mru_fixed.py",
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def read_payload(source):
    raw = source.read_bytes()
    values = {}
    for node in ast.parse(raw, filename=str(source)).body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id in {"PAYLOAD", "PAYLOAD_SHA256"}:
                    values[target.id] = ast.literal_eval(node.value)
    payload = base64.b64decode("".join(values["PAYLOAD"].split()), validate=True)
    if sha256(payload) != values["PAYLOAD_SHA256"]:
        raise ValueError(f"Nesouhlasí kontrolní součet payloadu: {source}")

    files = {}
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
        for member in archive.getmembers():
            name = PurePosixPath(member.name)
            if name.is_absolute() or ".." in name.parts or not name.parts:
                raise ValueError(f"Neplatná cesta v archivu: {member.name}")
            if not (member.isfile() or member.isdir()):
                raise ValueError(f"Nepodporovaný typ položky: {member.name}")
            if member.isfile():
                key = name.as_posix()
                if key in files:
                    raise ValueError(f"Duplicitní soubor: {key}")
                with archive.extractfile(member) as content:
                    files[key] = content.read()

    manifest = {
        "source_installer": source.relative_to(ROOT).as_posix(),
        "source_installer_sha256": sha256(raw),
        "payload_sha256": sha256(payload),
        "format": "base64 -> gzip -> tar -> Rust source project",
        "files": {name: sha256(data) for name, data in sorted(files.items())},
    }
    files["PAYLOAD-PUVOD.json"] = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode()
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Pouze ověří shodu kopií s payloady")
    args = parser.parse_args()

    pending = []
    for name, relative in SOURCES.items():
        destination = ROOT / "rozbalene-zdroje" / name
        files = read_payload(ROOT / relative)
        for filename, content in files.items():
            target = destination / filename
            if any(p.is_symlink() for p in (target, *target.parents) if p != ROOT and ROOT in p.parents):
                raise ValueError(f"Cíl nesmí vést přes symbolický odkaz: {target}")
            if target.exists():
                if not target.is_file() or target.read_bytes() != content:
                    raise ValueError(f"Kopie se liší; nepřepisuji: {target}")
            elif args.check:
                raise ValueError(f"Chybí kopie: {target}")
            else:
                pending.append((target, content))
        if destination.exists():
            extra = {p.relative_to(destination).as_posix() for p in destination.rglob("*") if not p.is_dir()} - files.keys()
            if extra:
                raise ValueError(f"Neočekávané soubory v {destination}: {sorted(extra)}")
        print(f"{name}: ověřeno {len(files) - 1} souborů a původ payloadu")

    for target, content in pending:
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as output:
            output.write(content)
    print(f"Hotovo. Nově zapsaných souborů: {len(pending)}. Instalátory nebyly spuštěny.")


if __name__ == "__main__":
    main()
