"""Local installer checks. No build, service or real installation is executed."""
import importlib.util
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SOURCE = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('alt_tab_install', SOURCE / 'install.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallerTests(unittest.TestCase):
    def test_mac_rejected_before_creating_installation_directory(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder) / 'must-not-be-created'
            with mock.patch.object(sys, 'platform', 'darwin'), \
                    mock.patch.object(sys, 'argv', ['install.py']), \
                    mock.patch.object(installer, 'BASE', base), \
                    mock.patch.object(installer, 'run') as run:
                with self.assertRaisesRegex(RuntimeError, 'pouze pro Linux'):
                    installer.main()
                self.assertFalse(base.exists())
                run.assert_not_called()

    def build_context(self):
        # Only Linux preflight is mocked; Cargo/systemd never execute.
        stack = __import__('contextlib').ExitStack()
        stack.enter_context(mock.patch.object(installer, 'require_linux'))
        stack.enter_context(mock.patch.dict(os.environ, {
            'WAYLAND_DISPLAY': 'wayland-test', 'XDG_RUNTIME_DIR': '/unused-test-runtime',
        }))
        stack.enter_context(mock.patch.object(installer.shutil, 'which', return_value='/mock/tool'))
        stack.enter_context(mock.patch.object(installer.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)))
        return stack

    def test_check_only_tests_and_builds_without_installing(self):
        with self.build_context(), \
                mock.patch.object(installer, 'run', return_value=subprocess.CompletedProcess([], 0, stdout='cosmic-ext-app-switcher 1.4.0\n')) as run, \
                mock.patch.object(installer, 'install_binary') as install_binary, \
                mock.patch.object(installer, 'idle_ui') as idle_ui:
            installer.install(SOURCE, check_only=True)
            commands = [call.args[0] for call in run.call_args_list]
            self.assertTrue(any(command[1:3] == ['test', '--locked'] for command in commands))
            self.assertTrue(any(command[1:4] == ['build', '--locked', '--release'] for command in commands))
            self.assertTrue(any(command[-1] == '--check-compat' for command in commands))
            install_binary.assert_not_called()
            idle_ui.assert_not_called()

    def test_failed_tests_do_not_replace_installed_binary(self):
        def run(command, **kwargs):
            if command[1] == 'test':
                raise subprocess.CalledProcessError(1, command)
            return subprocess.CompletedProcess(command, 0)

        with self.build_context(), mock.patch.object(installer, 'run', side_effect=run), \
                mock.patch.object(installer, 'backup') as backup, \
                mock.patch.object(installer, 'atomic_bytes') as write:
            with self.assertRaises(subprocess.CalledProcessError):
                installer.install(SOURCE)
            backup.assert_not_called()
            write.assert_not_called()

    def test_wrong_build_version_does_not_install(self):
        with self.build_context(), \
                mock.patch.object(installer, 'run', return_value=subprocess.CompletedProcess([], 0, stdout='cosmic-ext-app-switcher 0.1.5\n')), \
                mock.patch.object(installer, 'install_binary') as install_binary:
            with self.assertRaisesRegex(RuntimeError, 'neocekavana verze'):
                installer.install(SOURCE)
            install_binary.assert_not_called()

    def test_live_legacy_ui_is_not_replaced(self):
        with tempfile.TemporaryDirectory(prefix='at-') as folder, \
                mock.patch.dict(os.environ, {'XDG_RUNTIME_DIR': folder}):
            # Short /tmp socket names also fit macOS's Unix socket path limit.
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
                path = str(Path(folder) / 'cosmic-ext-app-switcher.sock')
                listener.bind(path)
                listener.listen()
                with self.assertRaisesRegex(RuntimeError, 'predchozi UI'):
                    with installer.idle_ui():
                        self.fail('Must refuse a live UI')
                self.assertTrue(Path(path).exists())

    def test_held_lifetime_lock_prevents_installation(self):
        with tempfile.TemporaryDirectory(prefix='at-') as folder, \
                mock.patch.dict(os.environ, {'XDG_RUNTIME_DIR': folder}):
            path = Path(folder) / 'cosmic-ext-app-switcher-ui.lock'
            with path.open('a') as owner:
                installer.fcntl.flock(owner, installer.fcntl.LOCK_EX | installer.fcntl.LOCK_NB)
                with self.assertRaisesRegex(RuntimeError, 'jeste otevreny'):
                    with installer.idle_ui():
                        self.fail('Must not install over a UI owner')

    def test_failed_install_restores_previous_files(self):
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'new-binary'
            binary.write_bytes(b'test executable data')
            backup = Path(folder) / 'backup'
            with mock.patch.object(installer, 'backup', return_value=backup), \
                    mock.patch.object(installer.subprocess, 'run'), \
                    mock.patch.object(installer, 'atomic_bytes', side_effect=OSError('disk full')), \
                    mock.patch.object(installer, 'restore') as restore:
                with self.assertRaisesRegex(OSError, 'disk full'):
                    installer.install_binary(binary)
                restore.assert_called_once_with(backup)


if __name__ == '__main__':
    unittest.main()
