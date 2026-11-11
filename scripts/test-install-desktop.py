#!/usr/bin/env python3
"""Exercise the installer using isolated XDG directories and fake desktop tools."""
import json
import os
from pathlib import Path
import pty
import select
import shutil
import subprocess
import tempfile
import time
import unittest

SCRIPT = Path(__file__).with_name('install-desktop.sh').resolve()

class DesktopInstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='crab-installer-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        for name in ['sh', 'awk', 'tr', 'sort', 'dirname', 'sed', 'grep', 'mkdir', 'mktemp', 'cat', 'chmod', 'mv', 'rm', 'rmdir', 'install']:
            (self.tools / name).symlink_to(shutil.which(name))
        validator = shutil.which('desktop-file-validate')
        if validator:
            (self.tools / 'desktop-file-validate').symlink_to(validator)
        self.tool('uname', '#!/bin/sh\nprintf "%s\\n" Linux\n')
        self.tool('id', '#!/bin/sh\nprintf "%s\\n" "$TEST_UID"\n')
        self.tool('xdg-mime', '''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
path = Path(os.environ['XDG_CONFIG_HOME']) / 'defaults.json'
data = json.loads(path.read_text())
if sys.argv[1] == 'default':
    data[sys.argv[3]] = sys.argv[2]
    path.write_text(json.dumps(data))
else:
    print(data.get(sys.argv[3], ''))
''')
        self.binary = self.root / 'install with spaces' / 'crab'
        self.binary.parent.mkdir()
        self.binary.write_text('#!/bin/sh\nexit 0\n')
        self.binary.chmod(0o755)
        self.env = os.environ.copy()
        self.env.update(PATH=str(self.tools), XDG_DATA_HOME=str(self.root / 'data'),
                        XDG_CONFIG_HOME=str(self.root / 'config'), TEST_UID='1000')
        Path(self.env['XDG_CONFIG_HOME']).mkdir()
        self.defaults = Path(self.env['XDG_CONFIG_HOME']) / 'defaults.json'
        self.original = {'audio/mpeg': 'elisa.desktop', 'audio/x-wav': 'elisa.desktop'}
        self.defaults.write_text(json.dumps(self.original))
        self.desktop = Path(self.env['XDG_DATA_HOME']) / 'applications' / 'crab.desktop'

    def tool(self, name, text):
        path = self.tools / name
        path.write_text(text)
        path.chmod(0o755)
        return path

    def run_script(self, mode='ask', staging='', types=''):
        return subprocess.run(['/bin/sh', str(SCRIPT), mode, str(self.binary), staging, types],
                              env=self.env, input='', text=True, capture_output=True, timeout=5)

    def assert_unchanged(self):
        self.assertFalse(self.desktop.exists())
        self.assertEqual(json.loads(self.defaults.read_text()), self.original)

    def interactive(self, answer):
        master, slave = pty.openpty()
        process = subprocess.Popen(['/bin/sh', str(SCRIPT), 'ask', str(self.binary), ''],
                                   env=self.env, stdin=slave, stdout=slave, stderr=slave)
        os.close(slave)
        output = b''
        try:
            deadline = time.monotonic() + 5
            sent = False
            while time.monotonic() < deadline:
                if select.select([master], [], [], .05)[0]:
                    try:
                        output += os.read(master, 65536)
                    except OSError:
                        break
                if not sent and b'[y/N]' in output:
                    os.write(master, answer)
                    sent = True
                if process.poll() is not None:
                    break
            process.wait(timeout=1)
            self.assertTrue(sent, output.decode(errors='replace'))
            return process.returncode, output.decode(errors='replace')
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)

    def test_noninteractive_default_and_explicit_no_leave_defaults_alone(self):
        for mode in ['ask', 'no']:
            self.assertEqual(self.run_script(mode).returncode, 0)
            self.assert_unchanged()

    def test_enter_and_no_decline_interactive_prompt(self):
        for answer in [b'\n', b'n\n']:
            code, output = self.interactive(answer)
            self.assertEqual(code, 0, output)
            self.assert_unchanged()

    def test_yes_registers_kitty_with_a_quoted_binary_path(self):
        kitty = self.tool('kitty', '#!/bin/sh\nexit 99\n')
        result = self.run_script('yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        text = self.desktop.read_text()
        self.assertIn('Terminal=false', text)
        self.assertIn('Exec="' + str(kitty) + '" --class crab --title CRAB "' + str(self.binary) + '" %F', text)
        defaults = json.loads(self.defaults.read_text())
        self.assertEqual(defaults['audio/mpeg'], 'crab.desktop')
        self.assertEqual(defaults['audio/x-wav'], 'elisa.desktop')

    def test_interactive_yes_and_invalid_answer_retry(self):
        code, output = self.interactive(b'maybe\ns\nmp3\n')
        self.assertEqual(code, 0, output)
        self.assertIn('Please answer yes or no.', output)
        self.assertTrue(self.desktop.exists())

    def test_no_kitty_uses_desktop_default_terminal(self):
        result = self.run_script('yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Terminal=true', self.desktop.read_text())
        self.assertIn('Exec="' + str(self.binary) + '" %F', self.desktop.read_text())

    def test_staged_and_root_installs_do_not_change_desktop_defaults(self):
        self.assertEqual(self.run_script('yes', '/package-root').returncode, 0)
        self.assert_unchanged()
        self.env['TEST_UID'] = '0'
        self.assertEqual(self.run_script('yes').returncode, 0)
        self.assert_unchanged()

    def test_missing_xdg_mime_and_invalid_mode_fail_without_changing_defaults(self):
        self.assertNotEqual(self.run_script('invalid').returncode, 0)
        (self.tools / 'xdg-mime').unlink()
        self.assertNotEqual(self.run_script('yes').returncode, 0)
        self.assert_unchanged()

    def test_exec_special_characters_are_escaped(self):
        self.binary = self.binary.with_name('crab cash$ percent% quote" slash\\ tick' + chr(96))
        self.binary.write_text('#!/bin/sh\nexit 0\n')
        self.binary.chmod(0o755)
        result = self.run_script('yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        line = next(line for line in self.desktop.read_text().splitlines() if line.startswith('Exec='))
        self.assertIn('percent%%', line)
        self.assertIn('cash\\\\$', line)
        self.assertIn('quote\\\\"', line)


    def make_install(self, staging=False):
        make = shutil.which('make')
        if not make:
            self.skipTest('make is unavailable')
        project = self.root / 'project'
        (project / 'scripts').mkdir(parents=True)
        (project / 'target' / 'release').mkdir(parents=True)
        shutil.copyfile(SCRIPT.parent.parent / 'Makefile', project / 'Makefile')
        shutil.copyfile(SCRIPT, project / 'scripts' / SCRIPT.name)
        shutil.copyfile(SCRIPT.with_name('audio-formats.tsv'), project / 'scripts' / 'audio-formats.tsv')
        shutil.copyfile(self.binary, project / 'target' / 'release' / 'crab')
        (project / 'target' / 'release' / 'crab').chmod(0o755)
        prefix = self.root / 'custom prefix'
        arguments = [make, 'install', 'PREFIX=' + str(prefix), 'ASSOCIATE=yes']
        destination = prefix / 'bin' / 'crab'
        if staging:
            package = self.root / 'package'
            arguments.append('DESTDIR=' + str(package))
            destination = package / str(destination).lstrip('/')
        result = subprocess.run(arguments, cwd=project, env=self.env, text=True,
                                capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(destination.is_file())
        return prefix / 'bin' / 'crab'

    def test_make_install_honors_custom_prefix_and_registers_installed_binary(self):
        installed = self.make_install()
        self.assertIn('Exec="' + str(installed) + '" %F', self.desktop.read_text())

    def test_make_install_destdir_preserves_desktop_defaults(self):
        self.make_install(staging=True)
        self.assert_unchanged()


    def test_only_selected_types_change_default_applications(self):
        result = self.run_script('yes', types=' MP3 , FLAC ')
        self.assertEqual(result.returncode, 0, result.stderr)
        defaults = json.loads(self.defaults.read_text())
        self.assertEqual(defaults['audio/mpeg'], 'crab.desktop')
        self.assertEqual(defaults['audio/flac'], 'crab.desktop')
        self.assertEqual(defaults['audio/x-wav'], 'elisa.desktop')

    def test_external_types_are_available_only_with_ffmpeg(self):
        self.assertNotEqual(self.run_script('yes', types='opus').returncode, 0)
        self.assert_unchanged()
        self.tool('ffmpeg', '#!/bin/sh\nexit 0\n')
        result = self.run_script('yes', types='opus')
        self.assertEqual(result.returncode, 0, result.stderr)
        defaults = json.loads(self.defaults.read_text())
        self.assertEqual(defaults['audio/opus'], 'crab.desktop')
        self.assertEqual(defaults['audio/x-wav'], 'elisa.desktop')
        self.assertEqual(defaults['audio/mpeg'], 'elisa.desktop')

    def test_all_and_none_selections(self):
        self.assertEqual(self.run_script('yes', types='none').returncode, 0)
        self.assert_unchanged()
        result = self.run_script('yes', types='all')
        self.assertEqual(result.returncode, 0, result.stderr)
        defaults = json.loads(self.defaults.read_text())
        self.assertEqual(defaults['audio/x-wav'], 'crab.desktop')
        self.assertEqual(defaults['audio/x-aiff'], 'crab.desktop')
        self.assertNotIn('audio/opus', defaults)

    def test_interactive_menu_can_choose_mp3_without_wav(self):
        code, output = self.interactive(b'y\nmp3\n')
        self.assertEqual(code, 0, output)
        defaults = json.loads(self.defaults.read_text())
        self.assertEqual(defaults['audio/mpeg'], 'crab.desktop')
        self.assertEqual(defaults['audio/x-wav'], 'elisa.desktop')


    def test_desktop_publication_does_not_create_transient_desktop_directories(self):
        real_mktemp = shutil.which('mktemp')
        (self.tools / 'mktemp').unlink()
        trace = self.root / 'temporary-paths.log'
        self.env['TEMP_TRACE'] = str(trace)
        self.tool('mktemp', '#!/bin/sh\ncreated=$("' + real_mktemp + '" "$@") || exit $?\nprintf "%s\\n" "$created" >> "$TEMP_TRACE"\nprintf "%s\\n" "$created"\n')
        for _ in range(2):
            result = self.run_script('yes', types='mp3')
            self.assertEqual(result.returncode, 0, result.stderr)
        paths = [Path(line) for line in trace.read_text().splitlines()]
        self.assertEqual(len(paths), 4)
        applications = self.desktop.parent
        for validation, staged in zip(paths[::2], paths[1::2]):
            self.assertNotIn(applications, validation.parents)
            self.assertEqual(staged.parent, applications)
            self.assertEqual(staged.suffix, '.tmp')
            self.assertFalse(validation.exists())
            self.assertFalse(staged.exists())
        self.assertEqual(list(applications.iterdir()), [self.desktop])
        self.assertIn('Exec=', self.desktop.read_text())

    def test_failed_validation_preserves_existing_launcher(self):
        self.desktop.parent.mkdir(parents=True)
        self.desktop.write_text('existing launcher')
        validator = self.tools / 'desktop-file-validate'
        if validator.exists():
            validator.unlink()
        self.tool('desktop-file-validate', '#!/bin/sh\nexit 1\n')
        result = self.run_script('yes', types='mp3')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.desktop.read_text(), 'existing launcher')
        self.assertEqual(json.loads(self.defaults.read_text()), self.original)
        self.assertEqual(list(self.desktop.parent.iterdir()), [self.desktop])

if __name__ == '__main__':
    unittest.main()
