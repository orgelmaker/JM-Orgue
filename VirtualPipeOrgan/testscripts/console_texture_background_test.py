"""Test saved console textures in owned headless child processes, without UI."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen

PURPOSE = 'jm-orgue-console-background-test'
FILES = ('Console A.organ', 'Console B.organ')


def norm(value):
    return os.path.normcase(str(Path(value).resolve()))


def read_json(file):
    return json.loads(file.read_text(encoding='utf-8-sig'))


def saved_layout(library, organ):
    return next((s.get('console_layout') for key, s in library['settings'].items()
                 if norm(key) == norm(organ)), None)


def digest(file):
    return hashlib.sha256(file.read_bytes()).hexdigest()


def json_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


class Child:
    def __init__(self, exe, profile, log):
        with socket.socket() as probe:
            probe.bind(('127.0.0.1', 0))
            self.port = probe.getsockname()[1]
        self.log = log.open('ab')
        self.process = None
        try:
            self.process = subprocess.Popen(
                [str(exe), '--test-headless', f'--test-api={self.port}', '--test-data-dir', str(profile)],
                stdin=subprocess.DEVNULL, stdout=self.log, stderr=self.log,
                creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        except BaseException:
            self.log.close()
            raise
        self.verified = False
        deadline = time.monotonic() + 45
        try:
            while time.monotonic() < deadline:
                if self.process.poll() is not None:
                    raise AssertionError(f'Headless child stopped; inspect {log}')
                try:
                    actual = self.api('/test/profile', timeout=1)
                except (URLError, TimeoutError, ConnectionError):
                    time.sleep(.2)
                    continue
                assert actual['process_id'] == self.process.pid, 'API is not the owned child'
                assert norm(actual['app_data_dir']) == norm(profile), 'Child profile differs'
                self.verified = True
                return
            raise TimeoutError(f'Child API did not start; inspect {log}')
        except BaseException:
            self.stop()
            raise

    def api(self, route, body=None, expected=200, timeout=30):
        data = None if body is None else json.dumps(body).encode()
        request = Request(f'http://127.0.0.1:{self.port}{route}', data=data,
                          headers={'Content-Type': 'application/json'} if data else {})
        try:
            with urlopen(request, timeout=timeout) as response:
                status, value = response.status, json.load(response)
        except HTTPError as error:
            status, value = error.code, json.load(error)
        assert status == expected, f'{route}: HTTP {status}, expected {expected}: {value}'
        return value

    def layout(self, organ):
        return self.api('/settings/console_layout?organ_id=' + quote(str(organ), safe=''))['layout']

    def load(self, organ):
        self.api('/load_organ', {'path': str(organ)})
        assert norm(self.api('/organ')['id']) == norm(organ)

    def stop(self):
        # Never find/stop processes by name, PID files or API-supplied PIDs.
        try:
            if self.process is not None and self.process.poll() is None:
                try:
                    self.process.terminate()
                except OSError:
                    if self.process.poll() is None:
                        raise
                try:
                    self.process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=10)
        finally:
            self.log.close()


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
        sys.stderr.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('exe', 'source-dir', 'work-dir'):
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    assert all(p.is_absolute() for p in (args.exe, args.source_dir, args.work_dir)), 'Use absolute paths'
    exe, source, work = args.exe.resolve(strict=True), args.source_dir.resolve(strict=True), args.work_dir.resolve()
    assert exe.is_file() and exe.name.lower() in ('vpo-app.exe', 'vpo-app'), 'Explicit vpo-app executable required'
    assert source.as_posix().lower().endswith('/testen/consolebouwer-2026-11-26'), 'Only the synthetic source fixture is allowed'
    work.relative_to((source / 'achtergrond').resolve())
    assert (source / 'orgels' / '.jm-console-test-fixture').is_file()
    assert read_json(source / 'profile' / 'consolebouwer-test-profile.json')['purpose'] == 'jm-orgue-consolebouwer-live-test'
    marker = work / '.jm-console-background-test'
    if work.exists():
        assert marker.is_file() and read_json(marker) == {'purpose': PURPOSE, 'source': str(source)}, 'Unmarked work directory refused'
    else:
        work.mkdir(parents=True)
        marker.write_text(json.dumps({'purpose': PURPOSE, 'source': str(source)}), encoding='utf-8')
    lock = work / '.running'
    with lock.open('x', encoding='utf-8') as file:
        file.write(str(os.getpid()))
    child, result = None, {'status': 'running', 'checks': [], 'processes': [], 'snapshots': {}}
    def check(name):
        result['checks'].append(name)
        print('OK ' + name, flush=True)
    try:
        orgels, profile = work / 'orgels', work / 'profile'
        for directory in (orgels, profile):
            directory.mkdir(exist_ok=True)
            assert directory.resolve().parent == work, 'Linked output directory refused'
        sentinel = profile / 'consolebouwer-test-profile.json'
        if sentinel.exists():
            assert read_json(sentinel)['purpose'] == PURPOSE
        else:
            sentinel.write_text(json.dumps({'purpose': PURPOSE}), encoding='utf-8')
        source_ids = [source / 'orgels' / name for name in FILES]
        ids = [orgels / name for name in FILES]
        source_library = read_json(source / 'profile' / 'organ_library.json')
        designs = [saved_layout(source_library, organ) for organ in source_ids]
        assert all(d and d['version'] == 1 for d in designs), 'GUI-saved A/B designs required'
        import base64
        png = (source / 'orgels' / 'eigen-testtextuur.png').read_bytes()
        expected_image = 'data:image/png;base64,' + base64.b64encode(png).decode('ascii')
        assert designs[0]['texture'] == 'custom' and designs[0]['custom_texture']['data_url'] == expected_image
        assert designs[0] != designs[1], 'A/B designs must differ'
        source_sidecars = [read_json(file) for file in (source / 'orgels').glob('.jm-settings-organ-*.json')]
        for organ, design in zip(source_ids, designs):
            copies = [item for item in source_sidecars if item['naam'] == organ.name]
            assert len(copies) == 1 and copies[0]['instellingen']['console_layout'] == design
        check('GUI-saved A/B layouts match their separate source sidecars')
        result['source_layout_hashes'] = {name: json_hash(design) for name, design in zip(FILES, designs)}
        result['embedded_png_sha256'] = hashlib.sha256(png).hexdigest()
        for file in (source / 'orgels').iterdir():
            if file.suffix.lower() not in ('.organ', '.wav'):
                continue
            assert file.is_file() and not file.is_symlink()
            target = orgels / file.name
            assert target.resolve().parent == orgels
            if target.exists():
                assert digest(target) == digest(file), 'Changed existing fixture refused'
            else:
                shutil.copyfile(file, target)
        assert all(organ.is_file() for organ in ids)
        assert not any(file.suffix.lower() in ('.png', '.jpg', '.jpeg', '.webp') for file in orgels.iterdir())
        library_file = profile / 'organ_library.json'
        if library_file.exists():
            existing = read_json(library_file)
            assert all(norm(key) in [norm(p) for p in ids] for key in existing['settings']), 'Foreign profile settings refused'
            assert all(norm(entry['id']) in [norm(p) for p in ids] for entry in existing['organs']), 'Foreign library entries refused'
        check('GUI source designs read; only synthetic organ/WAV files copied, no PNG or sidecars')
        def snapshot(label):
            library = read_json(library_file)
            portable = {p.name: (p, read_json(p)) for p in orgels.glob('.jm-settings-organ-*.json')}
            for organ, design in zip(ids, designs):
                assert saved_layout(library, organ) == design
                copies = [(p, data) for p, data in portable.values() if data['naam'] == organ.name]
                assert len(copies) == 1 and copies[0][1]['versie'] == 1 and copies[0][1]['soort'] == 'bestand'
                assert copies[0][1]['instellingen']['console_layout'] == design
            active_id = child.api('/organ')['id']
            result['snapshots'][label] = {'library': digest(library_file), 'sidecars': {name: digest(p) for name, (p, _) in portable.items()},
                                          'api': {active_id: json_hash(child.layout(Path(active_id)))}}
        for run in (1, 2):
            child = Child(exe, profile, work / f'headless-child-{run}.log')
            result['processes'].append({'pid': child.process.pid, 'port': child.port})
            time.sleep(6)
            assert child.process.poll() is None, 'Headless child exited after startup'
            alive = child.api('/test/profile')
            assert alive['process_id'] == child.process.pid and norm(alive['app_data_dir']) == norm(profile)
            check(f'run {run}: headless process remains available after six seconds, PID {child.process.pid}')
            if run == 1:
                for organ, design in zip(ids, designs):
                    child.load(organ)
                    response = child.api('/settings/console_layout', {'organ_id': str(organ), 'layout': design})
                    assert not response.get('warning')
                    assert child.layout(organ) == design
                snapshot('seeded')
            else:
                assert result['processes'][0]['pid'] != child.process.pid
            for index in (0, 1, 0):
                child.load(ids[index])
                assert child.layout(ids[index]) == designs[index]
                result.setdefault('api_layout_hashes', []).append({'run': run, 'organ': FILES[index], 'sha256': json_hash(child.layout(ids[index]))})
                if index == 0:
                    assert child.layout(ids[0])['custom_texture']['data_url'] == expected_image
            child.api('/settings/save', {})
            snapshot(f'run-{run}')
            check(f'run {run}: A → B → A preserves exact independent layouts and embedded image, PID {child.process.pid}')
            if run == 1:
                child.api('/settings/console_layout', {'organ_id': str(ids[1]), 'layout': designs[1]}, expected=400)
                invalid = json.loads(json.dumps(designs[0])); invalid['custom_texture']['data_url'] = 'data:image/svg+xml;base64,PHN2Zy8+'
                child.api('/settings/console_layout', {'organ_id': str(ids[0]), 'layout': invalid}, expected=400)
                assert child.layout(ids[0]) == designs[0]
                snapshot('rejections')
                assert result['snapshots']['rejections'] == result['snapshots']['run-1']
                check('wrong-organ and invalid SVG writes rejected without changing A or B')
            child.api('/settings/save', {})
            child.stop(); child = None
        assert [saved_layout(read_json(source / 'profile' / 'organ_library.json'), p) for p in source_ids] == designs
        assert (source / 'orgels' / 'eigen-testtextuur.png').read_bytes() == png
        check('source GUI designs and PNG remain unchanged; both owned child processes stopped')
        result['status'] = 'passed'
    except BaseException as error:
        result['status'], result['error'] = 'failed', str(error)
        raise
    finally:
        if child is not None:
            child.stop()
        result['count'] = len(result['checks'])
        (work / 'background-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        summary = f"{result['status']}: {result['count']} checks; PIDs {[p['pid'] for p in result['processes']]}\n" + '\n'.join(result['checks']) + '\n'
        (work / 'background-summary.txt').write_text(summary, encoding='utf-8')
        lock.unlink()
        print(summary, flush=True)


if __name__ == '__main__':
    main()
