"""
Project / folder dumper
- Optimized for AI context: excludes redundancy and LaTeX noise.
"""

from __future__ import annotations
from pathlib import Path
from datetime import datetime
import os
import sys

# =========================
# CONFIG (edit these only)
# =========================

# Root folder to dump
ROOT_DIR = r"/home/jirkadolezal/Documents/testuju rychlost linuxu/alt tab"

# Output file name
OUTPUT_FILENAME = "folder_dump.txt"

# If True, export the dump into ROOT_DIR
EXPORT_TO_ROOT_DIR = True

# 1. EXCLUDED DIRECTORIES
# Odstraněny složky jiných typů prací (pokud píšeš Master, bachelor/phd nepotřebuješ)
EXCLUDED_DIR_NAMES = {
    ".git", ".idea", ".vscode", "__pycache__", "build", "dist", "node_modules",
    "venv", "fig", "photos", "plots", "data", "sketch", "tikz", "rozbalene-zdroje"
    # "bachelor_thesis", # Odkomentuj, pokud píšeš diplomku
    # "phd_thesis",      # Odkomentuj, pokud píšeš diplomku
}

# 2. EXCLUDED EXTENSIONS
# LaTeX balast a soubory, které AI nepotřebuje ke čtení textu
EXCLUDED_EXTS = {
    ".png", ".jpg", ".jpeg", ".webp", ".gif", ".ico", ".pdf",
    ".aux", ".bbl", ".blg", ".log", ".out", ".toc", ".lof", ".lot", 
    ".fdb_latexmk", ".fls", ".synctex.gz", ".glo", ".ist", ".s房屋",
    ".bst", ".cls", ".sty", ".pyc", ".sh", ".sk", ".dvi", ".ps",
    ".exe", ".dll", ".zip", ".7z"
}

# 3. EXCLUDED SPECIFIC FILES
# Soubory, které dělají "bordel" v dumpu nebo jsou to systémové věci
EXCLUDED_FILES = {
    "folder_dump.txt",  # Klíčové: Skript nebude dumpovat sám sebe
    "basic_dump.py",    # Skript nebude dumpovat svůj zdroják
    ".nojekyll",
    ".gitignore",
    "Makefile"          # Pokud AI nemá řešit build, Makefile je zbytečný
}

# Behavior
INCLUDE_TREE = True
INCLUDE_FILE_CONTENTS = True
MAX_FILE_SIZE_MB = 0.05 # Pro text stačí 50KB, větší soubory jsou podezřelé
SKIP_BINARY_FILES = True
INCLUDE_DOTFILES = False # Tečky (skryté věci) většinou AI nepotřebuje
FILE_END_MARKER = "\n" + ("-" * 80) + "\n\n"

# =========================
# IMPLEMENTATION
# =========================

def normalize_rel_path(p: Path) -> str:
    return p.as_posix().lstrip("./")

def should_exclude_dir(dir_path: Path, rel_dir: Path) -> bool:
    if dir_path.name in EXCLUDED_DIR_NAMES:
        return True
    return False

def should_exclude_file(file_path: Path, rel_file: Path) -> bool:
    if file_path.name in EXCLUDED_FILES:
        return True
    if file_path.suffix.lower() in {e.lower() for e in EXCLUDED_EXTS}:
        return True
    try:
        size_mb = file_path.stat().st_size / (1024 * 1024)
        if size_mb > MAX_FILE_SIZE_MB:
            return True
    except OSError:
        return True
    return False

def looks_binary(data: bytes) -> bool:
    if b"\x00" in data: return True
    text_chars = bytearray({7, 8, 9, 10, 12, 13, 27} | set(range(0x20, 0x100)))
    nontext = data.translate(None, text_chars)
    return (len(nontext) / max(1, len(data))) > 0.30

def build_tree_lines(root: Path) -> list[str]:
    lines: list[str] = [f"ROOT: {root.resolve()}", ""]
    for dirpath, dirnames, filenames in os.walk(root):
        dir_path = Path(dirpath)
        rel_dir = dir_path.relative_to(root)
        
        pruned = [d for d in dirnames if should_exclude_dir(dir_path / d, (dir_path / d).relative_to(root))]
        for d in pruned: dirnames.remove(d)

        depth = 0 if rel_dir == Path(".") else len(rel_dir.parts)
        indent = "  " * depth
        lines.append(f"{indent}[D] {normalize_rel_path(rel_dir) if rel_dir != Path('.') else '.'}")

        for fn in sorted(filenames, key=lambda x: x.lower()):
            fp = dir_path / fn
            if should_exclude_file(fp, fp.relative_to(root)): continue
            lines.append(f"{indent}  [F] {fn}")
    return lines

def iter_files(root: Path):
    for dirpath, dirnames, filenames in os.walk(root):
        dir_path = Path(dirpath)
        pruned = [d for d in dirnames if should_exclude_dir(dir_path / d, (dir_path / d).relative_to(root))]
        for d in pruned: dirnames.remove(d)
        for fn in sorted(filenames, key=lambda x: x.lower()):
            fp = dir_path / fn
            if not should_exclude_file(fp, fp.relative_to(root)):
                yield fp, fp.relative_to(root)

def read_text_safely(path: Path) -> tuple[str | None, str]:
    try:
        data = path.read_bytes()
        if SKIP_BINARY_FILES and looks_binary(data): return None, "SKIPPED (binary)"
        for enc in ("utf-8", sys.getdefaultencoding(), "cp1250", "latin-1"):
            try: return data.decode(enc), f"OK ({enc})"
            except: continue
        return data.decode("utf-8", errors="replace"), "OK (replacement)"
    except Exception as e:
        return None, f"ERROR: {e}"

def main() -> None:
    root = Path(ROOT_DIR).expanduser().resolve()
    out_path = root / (OUTPUT_FILENAME if OUTPUT_FILENAME.endswith(".txt") else OUTPUT_FILENAME + ".txt")

    with out_path.open("w", encoding="utf-8", newline="\n") as f:
        f.write(f"FOLDER DUMP - {datetime.now()}\n{'='*80}\n")
        if INCLUDE_TREE:
            f.write("\nDIRECTORY TREE\n" + "-"*20 + "\n")
            f.write("\n".join(build_tree_lines(root)) + f"\n{'='*80}\n")
        
        if INCLUDE_FILE_CONTENTS:
            for abs_path, rel_path in iter_files(root):
                f.write(f"FILE: {rel_path.as_posix()}\n" + "-"*40 + "\n")
                text, note = read_text_safely(abs_path)
                f.write(text if text else f"[{note}]")
                f.write(FILE_END_MARKER)

    print(f"Dump exported to: {out_path}")

if __name__ == "__main__":
    main()