"""Exercise the Orgelbouwer in owned, isolated headless app processes.

Requires an explicit freshly built executable and a marked work directory below
the repository's testen/orgelbouwer-2026-11-26. Creates synthetic samples only;
never uses the user's samplesets, normal profile, app windows or input devices.
"""
import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import sys
import time
import uuid
import wave

from console_texture_background_test import Child, digest, json_hash, norm, read_json


PURPOSE = 'jm-orgue-organ-project-background-test'
PROJECT_ID = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/one'))
SECOND_PROJECT_ID = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/two'))
SOURCE_IDS = [str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/' + letter)) for letter in ('A', 'B')]


def write_fixture(path, data):
    """Never replace a changed file left by an earlier failed run."""
    if path.exists():
        assert path.is_file() and not path.is_symlink() and path.read_bytes() == data, \
            f'Changed fixture refused: choose a fresh work directory: {path}'
    else:
        path.write_bytes(data)


def make_wave(path, rate, frequency, stereo):
    import io
    out = io.BytesIO()
    with wave.open(out, 'wb') as audio:
        channels = 2 if stereo else 1
        audio.setparams((channels, 2, rate, 0, 'NONE', 'not compressed'))
        frames = bytearray()
        count = rate * 7
        for n in range(count):
            # A gentle attack avoids an onset transient dominating the FFT.
            envelope = min(1.0, n / (rate * .02))
            phase = 2 * math.pi * frequency * n / rate
            frames += struct.pack('<h', round(7000 * envelope * math.sin(phase)))
            if stereo:
                frames += struct.pack('<h', round(4900 * envelope * math.sin(phase + .2)))
        audio.writeframes(frames)
    write_fixture(path, out.getvalue())


def organ_text(letter, names, samples, stop_ids=None):
    stop_ids = stop_ids or list(range(1, len(names) + 1))
    lines = [
        '[Organ]', f'ChurchName=Orgelbouwer bron {letter}',
        'OrganBuilder=JM-Orgue synthetische achtergrondproef', 'HasPedals=N',
        'NumberOfManuals=1', 'NumberOfWindchestGroups=1',
        f'NumberOfRanks={len(names)}', 'NumberOfEnclosures=0', 'NumberOfTremulants=0',
        '[WindchestGroup001]', 'Name=Bronwind', 'NumberOfEnclosures=0', 'NumberOfTremulants=0',
        '[Manual001]', 'Name=Manuaal', 'NumberOfLogicalKeys=1',
        'NumberOfAccessibleKeys=1', 'FirstAccessibleKeyMIDINoteNumber=69',
        f'NumberOfStops={len(names)}', 'NumberOfCouplers=0',
        'NumberOfDivisionals=0', 'NumberOfTremulants=0',
    ]
    lines += [f'Stop{index:03}={sid:03}' for index, sid in enumerate(stop_ids, 1)]
    for index, (sid, name, sample) in enumerate(zip(stop_ids, names, samples), 1):
        lines += [
            f'[Stop{sid:03}]', f'Name={name}', 'NumberOfRanks=1', f'Rank001={index:03}',
            'FirstAccessiblePipeLogicalKeyNumber=1', 'NumberOfAccessiblePipes=1',
            f'[Rank{index:03}]', f'Name={name}', 'WindchestGroup=1',
            'FirstMidiNoteNumber=69', 'NumberOfLogicalPipes=1', 'Percussive=N',
            'HarmonicNumber=8', 'AmplitudeLevel=50', f'Pipe001={sample}',
        ]
    return ('\n'.join(lines) + '\n').encode('utf-8')


def fixtures(directory):
    directory.mkdir(exist_ok=True)
    assert directory.resolve() == directory and not directory.is_symlink()
    write_fixture(directory / '.jm-organ-project-fixture', (PURPOSE + '\n').encode())
    make_wave(directory / 'A-415-stereo.wav', 44100, 415.0, True)
    make_wave(directory / 'A-celeste-stereo.wav', 44100, 415.0 * 2 ** (10 / 1200), True)
    make_wave(directory / 'B-440-mono.wav', 48000, 440.0, False)
    a = directory / 'Bron A.organ'
    b = directory / 'Bron B.organ'
    ordinary = directory / 'Gewoon A controle.organ'
    a_names = ['Grondstem 8', 'Voix celeste 8']
    a_samples = ['A-415-stereo.wav', 'A-celeste-stereo.wav']
    write_fixture(a, organ_text('A', a_names, a_samples))
    write_fixture(b, organ_text('B', ['Grondstem 8'], ['B-440-mono.wav']))
    # The ordinary control collides with project stop IDs. Without clearing the
    # project's source map its raw A415 would incorrectly keep playing at 440.
    write_fixture(ordinary, organ_text('gewone controle', a_names, a_samples, [101, 102]))
    return a, b, ordinary


def project_for(paths, catalogs):
    return {
        'schema_version': 1, 'id': PROJECT_ID, 'name': 'Achtergrond hybride een',
        'target_a4_hz': 440.0, 'next_stop_id': 202, 'next_division_id': 3,
        'sources': [
            {'id': source_id, 'path': str(path), 'package_root': None,
             'fingerprint': catalog['fingerprint'], 'resources': [], 'perspective': None,
             'reference_a4_hz': reference, 'tuning_policy': 'normalize', 'tuning_cents': 0.0}
            for source_id, path, catalog, reference in zip(SOURCE_IDS, paths, catalogs, [415.0, 440.0])
        ],
        'divisions': [
            {'id': 1, 'name': 'Hoofdwerk', 'midi_channel': 2,
             'first_midi_note': 69, 'key_count': 1, 'pedal': False},
            {'id': 2, 'name': 'Zwelwerk', 'midi_channel': 3,
             'first_midi_note': 69, 'key_count': 1, 'pedal': False},
        ],
        'stops': [
            {'id': 101, 'source_id': SOURCE_IDS[0], 'source_stop_id': 1,
             'division_id': 1, 'name': 'Bron A Grondstem'},
            {'id': 102, 'source_id': SOURCE_IDS[0], 'source_stop_id': 2,
             'division_id': 1, 'name': 'Bron A Celeste'},
            {'id': 201, 'source_id': SOURCE_IDS[1], 'source_stop_id': 1,
             'division_id': 2, 'name': 'Bron B Grondstem'},
        ],
        'couplers': [],
    }


def stop_map(organ):
    return {stop['internal_stop_id']: stop for division in organ['divisions'] for stop in division['stops']}


def midi_map(child):
    return {mapping['division']: mapping for mapping in child.api('/midi/mapping')['mappings']}


def layout_for(organ, name, theme):
    stops = stop_map(organ)
    return {
        'version': 1, 'enabled': True, 'name': name, 'theme': theme,
        'pages': [{'id': 'page-1', 'name': 'Eigen speeltafel', 'width': 960, 'height': 640,
                   'elements': [
                       {'id': 'register-' + str(sid), 'kind': 'stop', 'target_id': stops[sid]['id'],
                        'label': '', 'x': 40 + 160 * index, 'y': 80, 'width': 120, 'height': 92,
                        'appearance': 'rocker-ivory' if index == 1 else 'drawknob'}
                       for index, sid in enumerate([101, 102, 201])]}],
    }


def quiet(child, organ_loaded=True):
    child.api('/panic', {})
    if organ_loaded:
        child.api('/stops/set', {'ids': []})
    child.api('/settings/master?db=-35', {})
    child.api('/reverb', {'mix': 0.0})
    child.api('/settings/wind?enabled=0', {})


def load(child, path):
    child.api('/load_organ', {'path': str(path)})
    organ = child.api('/organ')
    assert norm(organ['id']) == norm(path)
    quiet(child)
    return organ


def measure(child, organ, sid, expected_hz, output, result, label):
    stop = stop_map(organ)[sid]
    child.api('/panic', {})
    child.api('/stops/set', {'ids': [stop['id']]})
    time.sleep(.15)
    child.api('/record/start', {'path': str(output)})
    try:
        note = child.api('/notes/69/on?velocity=100', {})
        assert note['voices_triggered'] >= 1, 'No playable pipe was triggered'
        time.sleep(3.2)
    finally:
        child.api('/notes/69/off', {})
        recorded = child.api('/record/stop', {})
        child.api('/panic', {})
    hz = recorded['peak_hz']
    cents_error = 1200 * math.log2(hz / expected_hz) if hz > 0 else None
    result.setdefault('measurements', []).append({
        'label': label, 'stop': sid, 'expected_hz': expected_hz, 'peak_hz': hz,
        'cents_error': cents_error, 'rms': recorded['rms'],
        'frames_dropped': recorded['frames_dropped'], 'recording': str(output),
    })
    assert recorded['decoded_samples'] > 0 and recorded['rms'] > .00001, recorded
    assert cents_error is not None and abs(cents_error) < 2.0, \
        f'{label}: measured {hz:.4f}, expected {expected_hz:.4f} Hz ({cents_error} cents)'
    return hz


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
        sys.stderr.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, required=True)
    parser.add_argument('--work-dir', type=Path, required=True)
    args = parser.parse_args()
    assert args.exe.is_absolute() and args.work_dir.is_absolute(), 'Use absolute paths'
    exe = args.exe.resolve(strict=True)
    assert exe.is_file() and exe.name.lower() in ('vpo-app.exe', 'vpo-app')
    work = args.work_dir.resolve()
    test_root = (Path(__file__).resolve().parents[2] / 'testen' / 'orgelbouwer-2026-11-26').resolve()
    work.relative_to(test_root)
    marker = work / '.jm-organ-project-background-test'
    if work.exists():
        assert marker.is_file() and read_json(marker) == {'purpose': PURPOSE}, 'Unmarked work directory refused'
    else:
        work.mkdir(parents=True)
        marker.write_text(json.dumps({'purpose': PURPOSE}), encoding='utf-8')
    lock = work / '.running'
    with lock.open('x', encoding='utf-8') as file:
        file.write(str(os.getpid()))
    result = {'status': 'running', 'checks': [], 'processes': [], 'measurements': []}
    child = None
    def check(name):
        result['checks'].append(name)
        print('OK ' + name, flush=True)
    try:
        sources, profile, recordings = work / 'bronnen', work / 'profile', work / 'opnamen'
        for directory in (sources, profile, recordings):
            directory.mkdir(exist_ok=True)
            assert directory.resolve().parent == work and not directory.is_symlink(), 'Linked output refused'
        sentinel = profile / 'orgelbouwer-test-profile.json'
        write_fixture(sentinel, json.dumps({'purpose': PURPOSE}).encode())
        a, b, ordinary = fixtures(sources)
        originals = {path: path.read_bytes() for path in sources.iterdir() if path.is_file()}
        original_hashes = {path.name: hashlib.sha256(data).hexdigest() for path, data in originals.items()}
        result['source_hashes_before'] = original_hashes
        project_path = work / 'Hybride een.jmorgan'
        second_path = work / 'Hybride twee.jmorgan'
        child = Child(exe, profile, work / 'headless-child-1.log')
        result['processes'].append({'pid': child.process.pid, 'port': child.port})
        quiet(child, organ_loaded=False)
        assert child.api('/status')['audio_running'], 'Owned child has no active audio engine'
        catalogs = [child.api('/project/source', {'path': str(path), 'package_root': None}) for path in (a, b)]
        assert {stop['id'] for stop in catalogs[0]['stops']} == {1, 2}
        assert {stop['id'] for stop in catalogs[1]['stops']} == {1}
        check('two source definitions in one folder inspected independently, with overlapping source IDs')
        project = project_for([a, b], catalogs)
        saved = child.api('/project/save', {'path': str(project_path), 'project': project})
        assert saved['id'] == PROJECT_ID
        disk = read_json(project_path)
        for source, expected in zip(disk['sources'], [a, b]):
            assert not Path(source['path']).is_absolute(), 'Source path was not made portable'
            assert norm(project_path.parent / source['path']) == norm(expected)
        project = child.api('/project/read', {'path': str(project_path)})
        assert project == saved, 'Read differs from the saved project with resolved paths'
        assert all(source['resources'] for source in project['sources']), 'Selected sample stamps were not saved'
        check('project saved atomically and read back with relative source paths and persistent IDs')
        organ = load(child, project_path)
        stops_before = {sid: stop['id'] for sid, stop in stop_map(organ).items()}
        assert set(stops_before) == {101, 102, 201}
        assert len(set(stops_before.values())) == 3
        check('complete selected registers from both sources appear as distinct playable project targets')
        design = layout_for(organ, 'Hybride een console', 'walnut')
        applied = child.api('/settings/console_layout', {'organ_id': str(project_path), 'layout': design})
        assert not applied.get('warning')
        assert child.layout(project_path) == design

        mappings = midi_map(child)
        for name, channel in [('Hoofdwerk', 1), ('Zwelwerk', 2)]:
            assert mappings[name]['channel'] == channel, mappings
            assert mappings[name]['first_midi_note'] == mappings[name]['last_midi_note'] == 69, mappings
        check('project MIDI channels and key ranges apply at first load (model 1-based, engine 0-based)')

        child.api('/midi/mapping', {'division': 'Hoofdwerk', 'channel': 3, 'first': 69, 'last': 69})
        child.api('/settings/save', {})
        organ = load(child, project_path)
        assert midi_map(child)['Hoofdwerk']['channel'] == 3
        check('F2-style learned MIDI channel survives reloading an unchanged project model')

        hoofdwerk = next(division for division in project['divisions'] if division['id'] == 1)
        hoofdwerk['midi_channel'] = 5
        child.api('/project/save', {'path': str(project_path), 'project': project})
        # Deliberately save settings from the OLD running organ after changing
        # the disk model: reproduces the app autosave timing race.
        child.api('/settings/save', {})
        organ = load(child, project_path)
        assert midi_map(child)['Hoofdwerk']['channel'] == 4
        assert midi_map(child)['Zwelwerk']['channel'] == 2
        assert child.layout(project_path) == design
        check('edited project MIDI model wins over stale runtime autosave and previous learned channel')

        hoofdwerk['midi_channel'] = 2
        child.api('/project/save', {'path': str(project_path), 'project': project})
        child.api('/settings/save', {})
        organ = load(child, project_path)
        assert midi_map(child)['Hoofdwerk']['channel'] == 1
        assert child.layout(project_path) == design
        check('restoring the project MIDI model also wins after the same autosave race')

        child.api('/settings/auto_retune?on=1', {})
        measure(child, organ, 101, 440.0, recordings / 'normaliseer-A.mp3', result, 'A415 to A440, stereo 44.1 kHz')
        measure(child, organ, 201, 440.0, recordings / 'normaliseer-B.mp3', result, 'B440 to A440, mono 48 kHz')
        measure(child, organ, 102, 440.0 * 2 ** (10 / 1200), recordings / 'normaliseer-celeste.mp3', result, 'celeste retains +10 cents')
        check('recorded output confirms separate source normalization and preserved celeste detuning')

        original = copy.deepcopy(project)
        for source in original['sources']:
            source['tuning_policy'] = 'original'
        child.api('/project/save', {'path': str(project_path), 'project': original})
        organ = load(child, project_path)
        child.api('/settings/auto_retune?on=1', {})
        measure(child, organ, 101, 415.0, recordings / 'origineel-A.mp3', result, 'A stays 415 despite global auto-retune')
        measure(child, organ, 201, 440.0, recordings / 'origineel-B.mp3', result, 'B stays 440 independently')
        check('original policy preserves both source references even with global auto-retune enabled')

        # Source list order and destination manual change must not identify a stop.
        project['sources'].reverse()
        project['stops'].reverse()
        next(stop for stop in project['stops'] if stop['id'] == 101)['division_id'] = 2
        child.api('/project/save', {'path': str(project_path), 'project': project})
        organ = load(child, project_path)
        assert {sid: stop['id'] for sid, stop in stop_map(organ).items()} == stops_before
        assert next(div['name'] for div in organ['divisions'] if any(stop['internal_stop_id'] == 101 for stop in div['stops'])) == 'Zwelwerk'
        assert child.layout(project_path) == design
        check('source/register order and a changed destination manual retain the same console target IDs')

        other = copy.deepcopy(project)
        other['id'], other['name'] = SECOND_PROJECT_ID, 'Achtergrond hybride twee'
        child.api('/project/save', {'path': str(second_path), 'project': other})
        other_organ = load(child, second_path)
        assert child.layout(second_path) is None, 'Second project inherited first project console'
        second_design = layout_for(other_organ, 'Hybride twee console', 'ivory')
        second_design['pages'][0]['elements'][0]['x'] = 120
        child.api('/settings/console_layout', {'organ_id': str(second_path), 'layout': second_design})
        child.api('/settings/save', {})
        load(child, project_path)
        assert child.layout(project_path) == design
        child.api('/settings/save', {})
        sidecars = list(work.glob('.jm-settings-project-*.json'))
        assert len(sidecars) == 2, f'Expected two UUID project sidecars, found {sidecars}'
        check('two projects in one folder keep independent console layouts and UUID sidecars')

        disk_hash = digest(project_path)
        active_before = child.api('/organ')
        invalid = copy.deepcopy(project)
        invalid['stops'][1]['id'] = invalid['stops'][0]['id']
        child.api('/project/save', {'path': str(project_path), 'project': invalid}, expected=400)
        assert digest(project_path) == disk_hash
        wrong_identity = copy.deepcopy(project)
        wrong_identity['id'] = SECOND_PROJECT_ID
        child.api('/project/save', {'path': str(project_path), 'project': wrong_identity}, expected=400)
        assert digest(project_path) == disk_hash
        check('invalid project and wrong project UUID saves rejected without replacing the existing file')

        def unchanged_after_failure(label):
            current = child.api('/organ')
            assert current['id'] == active_before['id']
            assert stop_map(current) == stop_map(active_before)
            assert child.layout(project_path) == design
            assert digest(project_path) == disk_hash
            check(label)

        a.write_bytes(originals[a] + b'\n; deliberately changed fixture fingerprint\n')
        try:
            child.api('/load_organ', {'path': str(project_path)}, expected=500)
            child.api('/project/save', {'path': str(project_path), 'project': project}, expected=400)
            unchanged_after_failure('changed source fingerprint rejected; active organ, console and saved project unchanged')
        finally:
            a.write_bytes(originals[a])
        missing = sources / 'B-440-mono.wav'
        held = sources / 'B-440-mono.wav.held-by-test'
        assert not held.exists()
        missing.rename(held)
        try:
            child.api('/load_organ', {'path': str(project_path)}, expected=500)
            unchanged_after_failure('missing selected sample rejected before replacing the active organ')
        finally:
            held.rename(missing)

        # Force an actual ID collision with a normal organ to detect stale policy.
        ordinary_organ = load(child, ordinary)
        child.api('/settings/auto_retune?on=0', {})
        child.api('/temperament', {'name': 'Origineel', 'cents': [0] * 12, 'retune': False, 'fine': 0})
        measure(child, ordinary_organ, 101, 415.0, recordings / 'gewone-controle.mp3', result, 'ordinary organ clears project source tuning')
        check('ordinary organ with colliding internal IDs no longer receives the project tuning')
        load(child, project_path)
        assert child.layout(project_path) == design
        child.api('/settings/save', {})
        child.stop()
        child = None
        check('first owned process stopped before the restart test')

        child = Child(exe, profile, work / 'headless-child-2.log')
        result['processes'].append({'pid': child.process.pid, 'port': child.port})
        assert result['processes'][0]['pid'] != child.process.pid
        organ = load(child, project_path)
        assert child.layout(project_path) == design
        assert {sid: stop['id'] for sid, stop in stop_map(organ).items()} == stops_before
        measure(child, organ, 101, 440.0, recordings / 'na-herstart-A.mp3', result, 'source tuning survives full process restart')
        load(child, second_path)
        assert child.layout(second_path) == second_design
        load(child, project_path)
        assert child.layout(project_path) == design
        check('restart restores project source tuning and exact independent A/B console layouts')
        result['source_hashes_after'] = {path.name: digest(path) for path in originals}
        assert result['source_hashes_after'] == original_hashes
        check('all synthetic source definitions and samples remain byte-for-byte unchanged')
        child.stop()
        child = None
        result['status'] = 'passed'
        check('both owned headless child processes stopped; no user app windows were controlled')
    except BaseException as error:
        result['status'], result['error'] = 'failed', str(error)
        raise
    finally:
        if child is not None:
            child.stop()
        result['count'] = len(result['checks'])
        (work / 'background-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        summary = f"{result['status']}: {result['count']} checks; PIDs {[p['pid'] for p in result['processes']]}\n"
        summary += '\n'.join(result['checks']) + '\n'
        (work / 'background-summary.txt').write_text(summary, encoding='utf-8')
        lock.unlink()
        print(summary, flush=True)


if __name__ == '__main__':
    main()
