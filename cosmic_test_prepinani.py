#!/usr/bin/env python3
"""Diagnostika přepínání oken COSMIC, Python 3.10+, pouze standardní knihovna.

Spuštění v terminálu uvnitř COSMIC (bez sudo):
    python3 cosmic_test_prepinani.py
Pouze kontrola bez přepínání oken:
    python3 cosmic_test_prepinani.py --jen-kontrola
Volitelná měření fyzických zkratek s již čitelným evdev zařízením:
    python3 cosmic_test_prepinani.py --klavesnice /dev/input/eventN

Nic neinstaluje, nestahuje, nekompiluje, nemění zkratky ani nastavení služeb.
Spouští existující cosmicmsg a cosmic-ws-switcher (napsané v Rustu).
Pokud daemon neběží, v aktivním režimu jej dočasně spustí jako vlastní proces
a po testu ho ukončí. Volba --bez-docasneho-daemonu toto vypne.
V aktivním testu přepíná otevřená okna, potom se pokusí obnovit původní fokus.
TXT + JSON zapisuje do nového adresáře vedle skriptu (nebo --vystup).

Metriky:
  command_ms: monotónní čas od Popen do dokončení komunikace s procesem.
  focus_event_ms: od téhož začátku do přijetí změny aktivního okna ze
    samostatného, trvale běžícího cosmicmsg --json subscribe.
  To není čas překreslení monitoru ani fyzická klávesa -> obraz.
  CLI dotazy nejsou použity k průběžnému pollingu fokusu během vzorku.
  Režie observeru, JSON, plánovače a Pythonu je součástí výsledku.

Rozhraní ověřená v dokumentaci projektů 2026-10-14:
  https://github.com/varbhat/cosmicmsg
  https://github.com/lucasdevtec/cosmic-ws-switcher
  https://github.com/j0rdiun/cosmic-ext-app-switcher
"""

import argparse
from collections import deque
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import selectors
import shutil
import signal
import statistics
import struct
import subprocess
import sys
import tempfile
import threading
import time
import traceback

VERSION = "1.0"
HOME = Path.home()
MAX_OUTPUT = 120_000


def scrub(value):
    """V JSON neukládáme titulky otevřených dokumentů nebo stránek."""
    if isinstance(value, dict):
        return {k: ("<titulek vynechán>" if k.lower() == "title" else scrub(v))
                for k, v in value.items()}
    if isinstance(value, list):
        return [scrub(v) for v in value]
    if isinstance(value, str):
        return value.replace(str(HOME), "~")
    return value


def finish_process(proc):
    if proc.poll() is None:
        try:
            os.killpg(proc.pid, signal.SIGTERM)
            proc.wait(timeout=0.4)
        except (ProcessLookupError, subprocess.TimeoutExpired):
            if proc.poll() is None:
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                proc.wait(timeout=1)


def run(argv, timeout=4):
    start = time.perf_counter_ns()
    result = {"argv": list(map(str, argv)), "start_ns": start, "returncode": None}
    try:
        # Temporary files avoid pipe deadlocks and unbounded in-memory output.
        with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
            proc = subprocess.Popen(argv, stdout=out, stderr=err,
                                    stdin=subprocess.DEVNULL, start_new_session=True)
            # Popen.wait(timeout=...) on POSIX uses progressively longer polling
            # sleeps. That would quantize a latency benchmark by tens of ms.
            # A blocking wait in a dedicated thread avoids that polling delay.
            done = threading.Event()
            end = []
            def reap():
                proc.wait()
                end.append(time.perf_counter_ns())
                done.set()
            waiter = threading.Thread(target=reap, daemon=True)
            waiter.start()
            try:
                if not done.wait(timeout):
                    result["timeout"] = True
            finally:
                finish_process(proc)
                waiter.join(timeout=1)
            result["returncode"] = proc.returncode
            result["command_ms"] = ((end[0] if end else time.perf_counter_ns()) - start) / 1e6
            for key, stream in (("stdout", out), ("stderr", err)):
                stream.seek(0)
                data = stream.read(MAX_OUTPUT + 1)
                result[key] = data[:MAX_OUTPUT].decode("utf-8", "replace")
                if len(data) > MAX_OUTPUT:
                    result[key] += "\n<zkráceno>"
    except OSError as exc:
        result["error"] = str(exc)
        result["command_ms"] = (time.perf_counter_ns() - start) / 1e6
    return result


def parsed(result):
    try:
        return json.loads(result.get("stdout", ""))
    except (ValueError, TypeError):
        return None


def compact_result(result, keep_text=False):
    result = dict(result)
    data = parsed(result)
    if data is not None:
        result.pop("stdout", None)
        result["data"] = scrub(data)
    elif not keep_text:
        output = result.pop("stdout", "")
        result["stdout_length"] = len(output)
        if output.strip() == "pong":
            result["stdout"] = "pong"
    return scrub(result)


def locate(name, extra=None):
    candidates = [shutil.which(name), str(HOME / ".local/bin" / name),
                  "/usr/local/bin/" + name, "/usr/bin/" + name]
    if extra:
        candidates.insert(0, str(Path(extra).expanduser().resolve()))
    for value in candidates:
        if value and Path(value).is_file() and os.access(value, os.X_OK):
            return value
    return None


def file_info(path):
    path = Path(path)
    try:
        stat = path.stat()
        info = {"path": str(path), "size": stat.st_size,
                "mtime": dt.datetime.fromtimestamp(stat.st_mtime).astimezone().isoformat()}
        if stat.st_size <= 250_000_000:
            digest = hashlib.sha256()
            with path.open("rb") as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(block)
            info["sha256"] = digest.hexdigest()
        return info
    except OSError as exc:
        return {"path": str(path), "error": str(exc)}


def statistics_ms(values):
    values = sorted(v for v in values if isinstance(v, (int, float)))
    if not values:
        return {"n": 0}
    return {"n": len(values), "min": round(values[0], 3),
            "median": round(statistics.median(values), 3),
            "p95": round(values[math.ceil(len(values) * .95) - 1], 3),
            "max": round(values[-1], 3)}


class FocusObserver:
    """One persistent stream; never polls the compositor for each sample."""
    def __init__(self, executable):
        self.events = deque(maxlen=10000)
        self.condition = threading.Condition()
        self.focus = None
        self.lines = 0
        self.bad_lines = 0
        self.errors = deque(maxlen=15)
        self.proc = subprocess.Popen([executable, "--json", "subscribe"],
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     stdin=subprocess.DEVNULL, start_new_session=True)
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.error_reader = threading.Thread(target=self._errors, daemon=True)
        self.reader.start()
        self.error_reader.start()

    def feed(self, data, stamp):
        if not isinstance(data, dict):
            return
        window = data.get("window")
        if data.get("type") not in ("window-opened", "window-updated") or not isinstance(window, dict):
            return
        states = window.get("state", [])
        identifier = window.get("identifier")
        if not isinstance(states, list) or not isinstance(identifier, str) or not identifier:
            return
        if "activated" in [str(s).lower() for s in states]:
            with self.condition:
                old = self.focus
                self.focus = identifier
                if old != identifier:
                    self.events.append({"ns": stamp, "identifier": identifier,
                                        "app_id": window.get("app_id"), "previous": old})
                    self.condition.notify_all()

    def _read(self):
        try:
            for line in self.proc.stdout:
                stamp = time.perf_counter_ns()
                self.lines += 1
                try:
                    self.feed(json.loads(line), stamp)
                except (ValueError, TypeError):
                    self.bad_lines += 1
        except OSError as exc:
            self.errors.append(str(exc))

    def _errors(self):
        try:
            for line in self.proc.stderr:
                self.errors.append(line.decode("utf-8", "replace")[:2000])
        except OSError:
            pass

    def wait_change(self, start, previous, timeout, target=None):
        deadline = start + int(timeout * 1e9)
        with self.condition:
            while True:
                for event in self.events:
                    if start <= event["ns"] <= deadline and event["identifier"] != previous:
                        if target is None or event["identifier"] == target:
                            return dict(event)
                remaining = (deadline - time.perf_counter_ns()) / 1e9
                if remaining <= 0 or self.proc.poll() is not None:
                    return None
                self.condition.wait(min(remaining, .1))

    def close(self):
        finish_process(self.proc)
        self.reader.join(timeout=1)
        self.error_reader.join(timeout=1)
        self.proc.stdout.close()
        self.proc.stderr.close()


def inventory(report, args):
    report["environment"] = {"python": sys.version, "kernel": platform.release(),
        "machine": platform.machine(), "session": {k: os.environ.get(k) for k in
        ("XDG_CURRENT_DESKTOP", "XDG_SESSION_TYPE", "WAYLAND_DISPLAY", "XDG_SESSION_DESKTOP")}}
    report["tools"] = {name: locate(name, args.switcher if name == "cosmic-ws-switcher" else
                                   args.cosmicmsg if name == "cosmicmsg" else None)
                       for name in ("cosmicmsg", "cosmic-ws-switcher", "cosmic-ext-app-switcher")}
    report["files"] = [file_info(p) for p in report["tools"].values() if p]
    checks = {}
    commands = {
        "service": ["systemctl", "--user", "show", "cosmic-ws-switcher.service",
                    "--property=ActiveState,SubState,MainPID,ExecStart,FragmentPath"],
        "service_unit": ["systemctl", "--user", "cat", "cosmic-ws-switcher.service"],
        "packages": ["dpkg-query", "-W", "-f=${Package} ${Version}\n",
                     "cosmic-comp", "cosmic-session", "cosmic-settings", "cosmic-launcher"],
    }
    for label, cmd in commands.items():
        checks[label] = compact_result(run(cmd), keep_text=True)
    report["checks"] = checks
    report["running_binaries"] = []
    proc_root = Path("/proc")
    if proc_root.exists():
        for proc in proc_root.iterdir():
            if not proc.name.isdigit():
                continue
            try:
                if proc.stat().st_uid != os.getuid():
                    continue
                exe = os.readlink(proc / "exe")
                name = Path(exe.removesuffix(" (deleted)")).name
                if name in report["tools"]:
                    info = file_info(proc / "exe")
                    info.update(pid=int(proc.name), executable=exe)
                    report["running_binaries"].append(info)
            except OSError:
                continue
    # Only known project paths, never a recursive home-directory scan.
    projects = [Path(args.projekt).expanduser()] if args.projekt else [HOME / "cosmic-ws-switcher"]
    report["source_indicators"] = []
    for project in projects:
        for mode in ("release", "debug"):
            binary = project / "target" / mode / "cosmic-ws-switcher"
            if binary.is_file():
                report["files"].append(file_info(binary))
        source = project / "src/main.rs"
        if source.is_file():
            content = source.read_text(errors="replace")[:250000]
            lines = content.splitlines()
            pattern = r"cosmicmsg_path|window_activate|collect_state|Command::|thread::sleep|Duration::|fn activate_window|fn send_command|read_timeout|write_timeout"
            matches = [{"line": i + 1, "text": line[:350]} for i, line in enumerate(lines)
                       if re.search(pattern, line)][:100]
            report["source_indicators"].append({"path": str(source), "matches": matches,
                "note": "Zdrojové řádky nedokazují, jaká verze je v běžící binárce."})
    report["shortcut_lines"] = []
    config = HOME / ".config/cosmic/com.system76.CosmicSettings.Shortcuts"
    if config.is_dir():
        for path in sorted(config.glob("v*/*")):
            if path.is_file() and path.stat().st_size < 200000:
                lines = path.read_text(errors="replace").splitlines()
                hits = [line[:1000] for line in lines if re.search(
                    r"cosmic-ws-switcher|cosmicmsg|cosmic-ext-app-switcher|WindowSwitcher|key:\s*\"(?:Left|Right|Tab)\"", line)]
                if hits:
                    report["shortcut_lines"].append({"path": str(path), "lines": hits})
    report["input_devices"] = []
    for path in Path("/sys/class/input").glob("event*/device/name"):
        try:
            name = path.read_text().strip()
            if re.search(r"keyboard|keychron|kbd", name, re.I):
                device = Path("/dev/input") / path.parents[1].name
                report["input_devices"].append({"path": str(device), "name": name,
                                                "readable": os.access(device, os.R_OK)})
        except OSError:
            pass


def benchmark(label, cmd, report, samples, timeout, observer=None, target=None):
    print("  " + label, flush=True)
    rows = []
    misses = 0
    # The first attempt is kept separately; no unreported warm-up switches.
    for index in range(samples):
        if observer is not None:
            time.sleep(.16)
        before = observer.focus if observer else None
        result = run(cmd, timeout)
        event = None
        if observer and result.get("returncode") == 0 and not result.get("timeout"):
            event = observer.wait_change(result["start_ns"], before, timeout, target)
        row = compact_result(result)
        row.update(index=index + 1, before=before)
        if observer:
            row["focus_event_ms"] = ((event["ns"] - result["start_ns"]) / 1e6) if event else None
            row["after"] = event["identifier"] if event else None
            if not event:
                misses += 1
            else:
                misses = 0
        rows.append(row)
        if result.get("returncode") != 0 or result.get("timeout") or misses >= 3:
            break
    report["benchmarks"].append({"label": label, "rows": rows,
        "command_stats_ms": statistics_ms([r["command_ms"] for r in rows]),
        "focus_stats_ms": statistics_ms([r.get("focus_event_ms") for r in rows]),
        "first_sample_separate": rows[0] if rows else None,
        "steady_command_stats_ms": statistics_ms([r["command_ms"] for r in rows[1:]]),
        "steady_focus_stats_ms": statistics_ms([r.get("focus_event_ms") for r in rows[1:]])})
    return rows


def active_identifier(windows):
    for window in windows:
        if isinstance(window, dict) and "activated" in window.get("state", []):
            return window.get("identifier")
    return None


def current_workspace_windows(windows, initial):
    current = next((w for w in windows if w.get("identifier") == initial), None)
    if not current:
        return []
    memberships = current.get("workspaces", [])
    # Conservative: a sticky/multi-workspace window is not a reliable anchor.
    if len(memberships) != 1 or not isinstance(memberships[0], str):
        return []
    return [w for w in windows if isinstance(w.get("identifier"), str) and w["identifier"]
            and w.get("workspaces") == memberships and "minimized" not in w.get("state", [])]


def manual_keys(path, observer, report, duration):
    """Opt-in evdev: record only Alt+Tab / Super+arrows, never typed text.

    Receive timestamps only: not hardware event timestamps. No grabs, permission
    changes or global listener installation. Non-matching keys are discarded.
    """
    if not observer:
        report["notes"].append("Klávesový test přeskočen: chybí observer cosmicmsg.")
        return
    device = Path(path).resolve()
    if not re.fullmatch(r"/dev/input/event\d+", str(device)):
        report["notes"].append("Klávesnice musí být konkrétní /dev/input/eventN.")
        return
    try:
        fd = os.open(device, os.O_RDONLY | os.O_NONBLOCK)
    except OSError as exc:
        report["notes"].append("Klávesový test není dostupný: " + str(exc))
        return
    print(f"\nPo dobu {duration} s zkoušej střídavě rychlé Alt+Tab a Super+šipky.", flush=True)
    print("Použij právě vybranou klávesnici. Ostatní stisky se neukládají.", flush=True)
    events = []
    modifiers = set()
    tab_press = None
    wire = struct.Struct("llHHI")
    buffer = b""
    selector = selectors.DefaultSelector()
    selector.register(fd, selectors.EVENT_READ)
    deadline = time.monotonic() + duration
    try:
        while time.monotonic() < deadline:
            if not selector.select(timeout=.1):
                continue
            try:
                data = os.read(fd, wire.size * 64)
            except BlockingIOError:
                continue
            if not data:
                break
            buffer += data
            while len(buffer) >= wire.size:
                _, _, kind, code, value = wire.unpack(buffer[:wire.size])
                buffer = buffer[wire.size:]
                stamp = time.perf_counter_ns()
                if kind == 0 and code == 3:  # SYN_DROPPED: state is no longer reliable
                    report["notes"].append("Klávesový vstup ztratil události; test ukončen.")
                    return
                if kind != 1:
                    continue
                if code in (56, 100, 125, 126):  # Alt and Super only
                    if value == 1:
                        modifiers.add(code)
                    elif value == 0:
                        modifiers.discard(code)
                        if code in (56, 100) and tab_press is not None:
                            events.append({"label": "Alt+Tab: puštění Alt", "ns": stamp,
                                           "before": tab_press[1]})
                            tab_press = None
                elif value == 1 and code == 15 and modifiers.intersection((56, 100)):
                    tab_press = (stamp, observer.focus)
                    events.append({"label": "Alt+Tab: stisk Tab", "ns": stamp,
                                   "before": observer.focus})
                elif value == 1 and code in (105, 106) and modifiers.intersection((125, 126)):
                    events.append({"label": "Super+" + ("Left" if code == 105 else "Right"),
                                   "ns": stamp, "before": observer.focus})
    finally:
        selector.close()
        os.close(fd)
        time.sleep(.3)
        with observer.condition:
            focus_events = list(observer.events)
        measured = []
        for i, event in enumerate(events):
            # Another shortcut invalidates causal attribution. Alt release is a
            # deliberate exception: both press and release can describe one switch.
            following = [e["ns"] for e in events[i + 1:] if e["label"] != "Alt+Tab: puštění Alt"]
            upper = min([event["ns"] + 2_000_000_000] + following)
            match = next((e for e in focus_events if event["ns"] <= e["ns"] < upper
                          and e["identifier"] != event["before"]), None)
            measured.append({**event, "focus_event_ms": (match["ns"] - event["ns"]) / 1e6
                             if match else None})
        report["manual_keys"] = {"device": str(device), "samples": measured,
            "note": "Orientační rozdíl časů přijetí evdev a Wayland události v Pythonu. "
                    "Plánování a bufferování obou vstupů mohou výsledek zkreslit. "
                    "Není to hardwarový input-to-photon benchmark; klikání myší test znehodnotí."}


def conclusions(report):
    notes = report["notes"]
    installed = next((f for f in report["files"]
                      if f["path"] == report["tools"].get("cosmic-ws-switcher")), {})
    for binary in report["running_binaries"]:
        if "cosmic-ws-switcher" in binary["executable"]:
            if installed.get("sha256") and binary.get("sha256"):
                if installed["sha256"] != binary["sha256"]:
                    notes.append("POTVRZENO: běžící switcher má jiný SHA-256 než nalezená instalovaná binárka. "
                                 "Může jít o starou verzi nebo službu spuštěnou z jiné cesty.")
                else:
                    notes.append("Běžící a nalezená instalovaná binárka switcheru jsou shodné (SHA-256).")
    for file in report["files"]:
        if "/target/release/cosmic-ws-switcher" in file["path"] and file.get("sha256") and installed.get("sha256"):
            notes.append("Release build a instalovaný switcher: " +
                         ("shodné." if file["sha256"] == installed["sha256"] else "LIŠÍ SE; build nemusí být nasazen."))
    for bench in report["benchmarks"]:
        rows = bench["rows"]
        failed = sum(r.get("returncode") != 0 or r.get("timeout", False) for r in rows)
        if failed:
            notes.append(f"{bench['label']}: {failed} neúspěšných příkazů; jejich časy nejsou úspěšná přepnutí.")
        if rows and "focus_event_ms" in rows[0]:
            missing = sum(r.get("focus_event_ms") is None for r in rows)
            if missing:
                notes.append(f"{bench['label']}: {missing}/{len(rows)} vzorků bez potvrzené změny fokusu. "
                             "Možné důvody: jediné okno, nefunkční přepínač, chybějící události nebo timeout. "
                             "Chybějící vzorek není nula ms.")
    notes.append("Doby různých testů nelze prostým odečtením rozdělit na přesné vnitřní fáze daemonu. "
                 "K tomu by bylo potřeba instrumentovat jeho konkrétní zdrojový kód.")


def write_report(directory, report):
    safe = scrub(report)
    (directory / "vysledky.json").write_text(json.dumps(safe, ensure_ascii=False, indent=2), encoding="utf-8")
    lines = ["COSMIC – DIAGNOSTIKA PŘEPÍNÁNÍ OKEN", "Verze skriptu: " + VERSION,
             "Čas: " + report["created"], "", "CO JE ZMĚŘENO",
             "Příkaz = start procesu až jeho konec (včetně režie Pythonu).",
             "Fokus = start příkazu až přijetí události změny aktivního okna.",
             "Fokus není čas vykreslení obrazu. Automatický test neprochází klávesovou zkratkou.",
             "Měření probíhá postupně; p95 z malého počtu vzorků je jen orientační.", "",
             "PŘEHLED (ms; pomlčka znamená nezměřeno)",
             f"{'Test':43} {'n':>3} {'příkaz med.':>12} {'fokus med.':>12} {'fokus p95':>12}"]
    for item in report["benchmarks"]:
        cmd = item["command_stats_ms"]
        focus = item["focus_stats_ms"]
        lines.append(f"{item['label'][:43]:43} {cmd['n']:>3} {str(cmd.get('median', '-')):>12} "
                     f"{str(focus.get('median', '-')):>12} {str(focus.get('p95', '-')):>12}")
    lines += ["", "ZJIŠTĚNÍ A OMEZENÍ"] + ["- " + n for n in report["notes"]]
    lines += ["", "DETAILY (včetně jednotlivých vzorků)",
              json.dumps(safe, ensure_ascii=False, indent=2)]
    (directory / "vysledky.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--jen-kontrola", action="store_true", help="nepřepínat okna")
    parser.add_argument("--bez-docasneho-daemonu", action="store_true", help="nespouštět ani dočasný daemon")
    parser.add_argument("--opakovani", type=int, default=8, help="vzorky na test, výchozí 8 (2–50)")
    parser.add_argument("--timeout", type=float, default=3, help="limit jednoho vzorku v sekundách (0.5–15)")
    parser.add_argument("--vystup", type=Path, help="rodičovský adresář pro nové výsledky")
    parser.add_argument("--projekt", help="cesta ke zdrojům cosmic-ws-switcher")
    parser.add_argument("--switcher", help="konkrétní spustitelná binárka cosmic-ws-switcher")
    parser.add_argument("--cosmicmsg", help="konkrétní spustitelná binárka cosmicmsg")
    parser.add_argument("--klavesnice", help="volitelný již čitelný /dev/input/eventN pro ruční test")
    parser.add_argument("--sekundy", type=int, default=25, help="trvání ručního testu kláves (5–120 s)")
    args = parser.parse_args()
    if not 2 <= args.opakovani <= 50 or not .5 <= args.timeout <= 15 or not 5 <= args.sekundy <= 120:
        parser.error("Neplatný rozsah --opakovani / --timeout / --sekundy.")
    if os.geteuid() == 0:
        parser.error("Spusť skript jako běžný uživatel v COSMIC, bez sudo.")
    parent = (args.vystup or Path(__file__).resolve().parent).expanduser().resolve()
    try:
        parent.mkdir(parents=True, exist_ok=True)
        directory = Path(tempfile.mkdtemp(prefix="cosmic-test-" + dt.datetime.now().strftime("%Y%m%d-%H%M%S") + "-", dir=parent))
        directory.chmod(0o700)
    except OSError as exc:
        parser.error(f"Nelze vytvořit adresář výsledků: {exc}. Použij --vystup ~/Documents")
    report = {"version": VERSION, "created": dt.datetime.now().astimezone().isoformat(),
              "benchmarks": [], "notes": [], "mode": "kontrola" if args.jen_kontrola else "aktivní test"}
    observer = None
    temporary_daemon = None
    daemon_log = None
    initial = None
    msg = None
    exit_code = 0
    print("COSMIC – test přepínání oken\nVýsledky: " + str(directory), flush=True)
    print("Nic se neinstaluje ani nepřenastavuje. Kontroluji existující programy…", flush=True)
    try:
        inventory(report, args)
        msg = report["tools"]["cosmicmsg"]
        switcher = report["tools"]["cosmic-ws-switcher"]
        true_cmd = shutil.which("true")
        if true_cmd:
            benchmark("Režie spuštění /usr/bin/true", [true_cmd], report, args.opakovani, args.timeout)
        ping_ok = False
        if switcher:
            rows = benchmark("Switcher: ping (IPC)", [switcher, "ping"], report, args.opakovani, args.timeout)
            ping_ok = bool(rows and rows[-1].get("returncode") == 0 and rows[-1].get("stdout", "").strip() == "pong")
            benchmark("Switcher: status", [switcher, "status"], report, min(3, args.opakovani), args.timeout)
        else:
            report["notes"].append("cosmic-ws-switcher nebyl nalezen; použij --switcher pro jinou cestu.")
        if not ping_ok:
            report["notes"].append("Původní daemon neodpověděl pong.")
            already_running = any("cosmic-ws-switcher" in p["executable"] for p in report["running_binaries"])
            if (switcher and not args.jen_kontrola and not args.bez_docasneho_daemonu
                    and not already_running and os.environ.get("WAYLAND_DISPLAY")):
                print("Daemon neběží. Spouštím jej dočasně jen pro test; po testu jej ukončím.", flush=True)
                daemon_log = tempfile.TemporaryFile()
                temporary_daemon = subprocess.Popen([switcher, "daemon"], stdout=daemon_log,
                    stderr=daemon_log, stdin=subprocess.DEVNULL, start_new_session=True)
                limit = time.monotonic() + 3
                while temporary_daemon.poll() is None and time.monotonic() < limit:
                    time.sleep(.15)
                    probe = run([switcher, "ping"], min(args.timeout, .5))
                    if probe.get("returncode") == 0 and probe.get("stdout", "").strip() == "pong":
                        ping_ok = True
                        break
                report["temporary_daemon"] = {"pid": temporary_daemon.pid, "ping_ok": ping_ok,
                    "note": "Dočasný proces diagnostiky, nikoli zapnutá systemd služba."}
                if ping_ok:
                    report["notes"].append("Měření rotování proběhlo s dočasně spuštěným daemonem z nalezené binárky.")
                    benchmark("Dočasný switcher: ping (IPC)", [switcher, "ping"], report,
                              args.opakovani, args.timeout)
            if not ping_ok:
                report["notes"].append("Daemon není dostupný; next/prev se přeskočí. "
                    "Existující neodpovídající proces se nerestartuje.")
        windows = []
        if msg and os.environ.get("WAYLAND_DISPLAY"):
            for command in ("get-toplevels", "get-workspaces"):
                result = run([msg, "--json", command], args.timeout)
                report["checks"][command] = compact_result(result)
                data = parsed(result)
                if command == "get-toplevels" and result.get("returncode") == 0 and isinstance(data, list):
                    windows = [w for w in data if isinstance(w, dict)]
            initial = active_identifier(windows)
            benchmark("Cosmicmsg: načtení oken", [msg, "--json", "get-toplevels"], report,
                      min(5, args.opakovani), args.timeout)
        else:
            report["notes"].append("Chybí cosmicmsg nebo WAYLAND_DISPLAY; skutečnou změnu fokusu nelze sledovat.")
        if not args.jen_kontrola and windows and initial:
            candidates = current_workspace_windows(windows, initial)
            if len(candidates) < 2:
                report["notes"].append("Nejsou spolehlivě rozpoznána alespoň dvě neminimalizovaná okna "
                                       "na ploše výchozího okna. Aktivní test přeskočen.")
            else:
                print("\nZa 5 sekund začne přepínání. Nech otevřená aspoň dvě okna na této ploše.", flush=True)
                print("Během automatického testu nepoužívej myš ani klávesnici; trvá zpravidla desítky sekund.", flush=True)
                time.sleep(5)
                observer = FocusObserver(msg)
                time.sleep(.5)
                # Re-read anchor after the preparation delay, before timing samples.
                fresh = parsed(run([msg, "--json", "get-toplevels"], args.timeout))
                if isinstance(fresh, list) and all(isinstance(w, dict) for w in fresh):
                    windows = fresh
                    initial = active_identifier(windows) or initial
                    candidates = current_workspace_windows(windows, initial)
                with observer.condition:
                    if observer.focus is None:
                        observer.focus = initial
                if len(candidates) >= 2 and observer.proc.poll() is None:
                    if ping_ok:
                        benchmark("Switcher: next → fokus", [switcher, "next"], report,
                                  args.opakovani, args.timeout, observer)
                        benchmark("Switcher: prev → fokus", [switcher, "prev"], report,
                                  args.opakovani, args.timeout, observer)
                    # Fixed identifiers from the original workspace, not ambiguous titles.
                    other = next(w["identifier"] for w in candidates if w["identifier"] != initial)
                    direct_rows = []
                    for i in range(args.opakovani):
                        target = other if observer.focus != other else initial
                        temporary = {"benchmarks": []}
                        rows = benchmark("Cosmicmsg: přímá aktivace " + str(i + 1),
                                         [msg, "window", "activate", target], temporary,
                                         1, args.timeout, observer, target)
                        direct_rows.extend(rows)
                        if rows[-1].get("returncode") != 0 or (len(direct_rows) >= 3 and
                                all(r.get("focus_event_ms") is None for r in direct_rows[-3:])):
                            break
                    report["benchmarks"].append({"label": "Cosmicmsg: activate → fokus", "rows": direct_rows,
                        "command_stats_ms": statistics_ms([r["command_ms"] for r in direct_rows]),
                        "focus_stats_ms": statistics_ms([r.get("focus_event_ms") for r in direct_rows])})
                else:
                    report["notes"].append("Během přípravy se změnila okna nebo skončil observer; aktivní test přeskočen.")
        if args.klavesnice and not args.jen_kontrola:
            if observer is None and msg and os.environ.get("WAYLAND_DISPLAY"):
                observer = FocusObserver(msg)
                time.sleep(.5)
            manual_keys(args.klavesnice, observer, report, args.sekundy)
        if not args.klavesnice:
            report["notes"].append("Fyzické Alt+Tab vs Super+šipky nejsou automaticky změřené. "
                "Volitelně --klavesnice /dev/input/eventN, pouze pokud má běžný uživatel přístup. "
                "Bez přístupu nic nepřenastavuj a nespouštěj celý skript přes sudo. "
                "Samotné spuštění grafického Alt+Tab programu z terminálu není srovnatelný test zkratky.")
    except KeyboardInterrupt:
        report["notes"].append("Test přerušen Ctrl+C; uloženy dosud dokončené testy.")
        exit_code = 130
    except Exception:
        report["error"] = traceback.format_exc()
        report["notes"].append("Test skončil chybou; detaily a částečné výsledky jsou v reportu.")
        exit_code = 1
    finally:
        if initial and msg and observer:
            report["restore_focus"] = compact_result(run([msg, "window", "activate", initial], args.timeout))
        if observer:
            observer.close()
            report["observer"] = {"lines": observer.lines, "invalid_json_lines": observer.bad_lines,
                "focus_changes": len(observer.events), "errors": list(observer.errors)}
            if not observer.events:
                report["notes"].append("Observer nezachytil žádnou změnu fokusu; časy aktivace nejsou ověřené.")
        if temporary_daemon:
            finish_process(temporary_daemon)
            report.setdefault("temporary_daemon", {})["stopped"] = temporary_daemon.poll() is not None
        if daemon_log:
            daemon_log.seek(0)
            report["temporary_daemon_log"] = daemon_log.read(16000).decode("utf-8", "replace")
            daemon_log.close()
        # Read only selected recent service errors; do not collect the complete journal.
        if shutil.which("journalctl"):
            logs = run(["journalctl", "--user", "-b", "-u", "cosmic-ws-switcher.service",
                        "--no-pager", "-n", "80", "-o", "cat"], 3)
            report["service_error_lines"] = [line[:1000] for line in logs.get("stdout", "").splitlines()
                if re.search(r"error|erro |failed|panic|No such file|timed out|timeout", line, re.I)]
        if "files" in report:
            conclusions(report)
        write_report(directory, report)
        print("\nHotovo. Nahraj sem tento soubor:\n" + str(directory / "vysledky.txt"), flush=True)
        print("Strojová data: " + str(directory / "vysledky.json"), flush=True)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
