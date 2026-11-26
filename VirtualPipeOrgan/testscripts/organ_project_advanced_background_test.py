"""Test independent rank enclosures and a 2048-stop project in owned headless apps.

Uses fresh synthetic fixtures and an isolated profile only. No user windows,
input devices, installed samplesets or production settings are operated.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
import time
import uuid

from console_texture_background_test import Child, digest, norm, read_json
from organ_project_background_test import fixtures, organ_text, project_for, quiet, stop_map


PURPOSE = 'jm-orgue-organ-project-advanced-background-test'
BOX_PROJECT_ID = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/boxes'))
LARGE_PROJECT_ID = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/2048'))


def inspect(child, path):
    return child.api('/project/source', {'path': str(path), 'package_root': None}, timeout=120)


def load(child, path):
    child.api('/load_organ', {'path': str(path)}, timeout=180)
    organ = child.api('/organ')
    assert norm(organ['id']) == norm(path)
    quiet(child)
    return organ


def add_rank_box(organ):
    config = organ.with_suffix('.acoustics.json')
    with organ.open('a', encoding='utf-8') as file:
        file.write(f'\n[JMOrgueAcoustics]\nFormat=jm-orgue.acoustics\nSchemaVersion=1\nConfigurationFile={config.name}\n')
    config.write_text(json.dumps({
        'format': 'jm-orgue.acoustics', 'schema_version': 1,
        'enclosures': [{'id': 'box', 'name': 'Zwelkast'}],
        'ranks': [{'id': 'rank1', 'name': 'Grondstem', 'enclosure_id': 'box'}],
        'pipe_bindings': [{'stop_section': 'Stop001', 'pipe_section': 'Pipe001',
                           'layer_index': 0, 'logical_pipe': 1, 'source_rank_id': 'rank1'}],
    }, indent=2), encoding='utf-8')
    return config


def box_slots(organ, project):
    summary = organ.get('acoustics')
    assert summary and summary['valid'], summary
    enclosures = summary['enclosures']
    assert len(enclosures) == 2 and len({e['id'] for e in enclosures}) == 2
    result = {}
    for index, source in enumerate(project['sources']):
        expected = f"project:{source['id']}:enclosure:box"
        enclosure = next(e for e in enclosures if e['id'] == expected)
        assert enclosure['swell_name'] == 'enclosure:' + expected
        result[index] = enclosure
    assert result[0]['slot'] != result[1]['slot']
    return result


def record_rms(child, organ, stop_id, path, result, label):
    stop = stop_map(organ)[stop_id]
    child.api('/panic', {})
    child.api('/stops/set', {'ids': [stop['id']]})
    time.sleep(.12)
    child.api('/record/start', {'path': str(path)})
    try:
        started = child.api('/notes/69/on?velocity=100', {})
        assert started['voices_triggered'] == 1, started
        time.sleep(.9)
    finally:
        child.api('/notes/69/off', {})
        recorded = child.api('/record/stop', {})
        child.api('/panic', {})
    assert recorded['decoded_samples'] > 0 and recorded['frames_dropped'] == 0, recorded
    result['audio'].append({'label': label, 'rms': recorded['rms'], 'peak_hz': recorded['peak_hz'],
                            'frames_dropped': recorded['frames_dropped'], 'recording': str(path)})
    return recorded['rms']


def set_box(child, enclosure, value):
    child.api('/swell', {'index': 128 + enclosure['slot'], 'gain': value})
    time.sleep(.12)


def layout_for_all(organ):
    stops = sorted(stop_map(organ).values(), key=lambda stop: stop['internal_stop_id'])
    pages = []
    for start in range(0, len(stops), 48):
        elements = []
        for index, stop in enumerate(stops[start:start + 48]):
            elements.append({'id': 'control-' + str(stop['internal_stop_id']), 'kind': 'stop',
                'target_id': stop['id'], 'label': '', 'x': 20 + (index % 8) * 140,
                'y': 40 + (index // 8) * 108, 'width': 120, 'height': 92, 'appearance': 'rocker'})
        pages.append({'id': 'page-' + str(len(pages) + 1), 'name': 'Registers ' + str(start + 1),
                      'width': 1200, 'height': 800, 'elements': elements})
    return {'version': 1, 'enabled': True, 'name': 'Volledig projectontwerp', 'theme': 'walnut', 'pages': pages}


def trigger_action(child, note, expected):
    child.api('/test/preset_triggers')
    child.api(f'/midi/inject?note={note}&on=1&velocity=100&channel=0', {})
    child.api(f'/midi/inject?note={note}&on=0&channel=0', {})
    seen = []
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        seen.extend(child.api('/test/preset_triggers')['triggers'])
        if expected in seen:
            assert seen == [expected], f'Wrong or duplicate wide MIDI action: {seen}, expected {expected}'
            return
        time.sleep(.04)
    raise AssertionError(f'Wide MIDI action {expected} did not arrive: {seen}')


def binding(code, note):
    return {'preset_num': code, 'trigger_type': 'note', 'note': note, 'channel': 0,
            'controller': None, 'value': None, 'program': None}


def play_edge(child, organ, sid):
    child.api('/panic', {})
    child.api('/stops/set', {'ids': [stop_map(organ)[sid]['id']]})
    started = child.api('/notes/69/on?velocity=60', {})
    assert started['voices_triggered'] == 1, started
    time.sleep(.1)
    assert child.api('/status')['voice_count'] > 0
    child.api('/notes/69/off', {})
    child.api('/panic', {})


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
        sys.stderr.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', required=True, type=Path)
    parser.add_argument('--work-dir', required=True, type=Path)
    args = parser.parse_args()
    assert args.exe.is_absolute() and args.work_dir.is_absolute()
    exe, work = args.exe.resolve(strict=True), args.work_dir.resolve()
    assert exe.name.lower() in ('vpo-app.exe', 'vpo-app') and exe.is_file()
    test_root = (Path(__file__).resolve().parents[2] / 'testen' / 'orgelbouwer-uitbreiding-2026-11-26').resolve()
    work.relative_to(test_root)
    assert work != test_root and not work.exists(), 'Use a fresh numbered work directory; existing files are never replaced'
    work.mkdir(parents=True)
    (work / '.jm-organ-project-advanced-test').write_text(json.dumps({'purpose': PURPOSE}), encoding='utf-8')
    lock = work / '.running'
    with lock.open('x', encoding='utf-8') as file:
        file.write(str(os.getpid()))
    result = {'status': 'running', 'checks': [], 'processes': [], 'audio': []}
    child = None
    originals = {}
    def check(name):
        result['checks'].append(name)
        print('OK ' + name, flush=True)
    try:
        sources, profile, recordings = work / 'bronnen', work / 'profile', work / 'opnamen'
        profile.mkdir(); recordings.mkdir()
        (profile / 'advanced-test-profile.json').write_text(json.dumps({'purpose': PURPOSE}), encoding='utf-8')
        a, b, _ = fixtures(sources)
        add_rank_box(a); add_rank_box(b)
        large_sources = work / 'grote-bronnen'
        large_sources.mkdir()
        large_paths = [large_sources / 'Groot A.organ', large_sources / 'Groot B.organ']
        for index, path in enumerate(large_paths):
            sample = '../bronnen/' + ('A-415-stereo.wav' if index == 0 else 'B-440-mono.wav')
            path.write_bytes(organ_text('groot ' + str(index + 1), [f'Register {n} 8' for n in range(1, 1025)], [sample] * 1024))
        all_sources = [path for directory in (sources, large_sources) for path in directory.iterdir() if path.is_file()]
        originals = {str(path): digest(path) for path in all_sources}
        result['source_hashes_before'] = originals
        box_path, large_path = work / 'Twee rankzwelkasten.jmorgan', work / '2048 registers.jmorgan'
        child = Child(exe, profile, work / 'headless-child-1.log')
        result['processes'].append({'pid': child.process.pid, 'port': child.port})
        assert child.api('/status')['audio_running']
        catalogs = [inspect(child, path) for path in (a, b)]
        boxes = project_for([a, b], catalogs)
        boxes['id'], boxes['name'] = BOX_PROJECT_ID, 'Onafhankelijke rankzwelkasten'
        next(stop for stop in boxes['stops'] if stop['id'] == 201)['division_id'] = 1
        child.api('/project/save', {'path': str(box_path), 'project': boxes})
        organ = load(child, box_path)
        slots = box_slots(organ, boxes)
        assert any({101, 201}.issubset({stop['internal_stop_id'] for stop in division['stops']}) for division in organ['divisions'])
        check('two source-local box/rank IDs remain independent on the same project manual')
        box_layout = layout_for_all(organ)
        child.api('/settings/console_layout', {'organ_id': str(box_path), 'layout': box_layout})
        for index, enclosure in slots.items():
            child.api('/swell/binding', {'division': enclosure['swell_name'], 'channel': 0, 'cc': 11 + index})
            set_box(child, enclosure, 1.0)
        a_open = record_rms(child, organ, 101, recordings / 'A-open.mp3', result, 'A open')
        b_open = record_rms(child, organ, 201, recordings / 'B-open.mp3', result, 'B open')
        assert a_open > .00001 and b_open > .00001
        set_box(child, slots[0], 0.0)
        a_closed = record_rms(child, organ, 101, recordings / 'A-dicht.mp3', result, 'A closed')
        b_when_a_closed = record_rms(child, organ, 201, recordings / 'B-A-dicht.mp3', result, 'B while A closed')
        assert a_closed < a_open * .35, (a_open, a_closed)
        assert abs(b_when_a_closed / b_open - 1) < .05, (b_open, b_when_a_closed)
        check('closing source A attenuates A while source B retains its measured level')
        set_box(child, slots[0], 1.0); set_box(child, slots[1], 0.0)
        b_closed = record_rms(child, organ, 201, recordings / 'B-dicht.mp3', result, 'B closed')
        a_when_b_closed = record_rms(child, organ, 101, recordings / 'A-B-dicht.mp3', result, 'A while B closed')
        assert b_closed < b_open * .35, (b_open, b_closed)
        assert abs(a_when_b_closed / a_open - 1) < .05, (a_open, a_when_b_closed)
        check('closing source B attenuates B while source A retains its measured level')
        for index in slots:
            child.api(f'/midi/inject?cc={11 + index}&value=127&channel=0', {})
        time.sleep(.15)
        child.api('/settings/save', {})
        result['box_bindings'] = child.api('/settings/organ')['swell_bindings']
        assert {b['division_name'] for b in result['box_bindings']} == {slot['swell_name'] for slot in slots.values()}
        check('independent enclosure CC bindings and console design are saved by project identity')

        large_catalogs = [inspect(child, path) for path in large_paths]
        assert all(len(catalog['stops']) == 1024 for catalog in large_catalogs)
        large = project_for(large_paths, large_catalogs)
        large['id'], large['name'], large['next_stop_id'] = LARGE_PROJECT_ID, 'Superorgel met 2048 registers', 2049
        large['stops'] = [{'id': index + 1, 'source_id': large['sources'][index // 1024]['id'],
                          'source_stop_id': index % 1024 + 1, 'division_id': index // 1024 + 1,
                          'name': None} for index in range(2048)]
        child.api('/project/save', {'path': str(large_path), 'project': large}, timeout=180)
        large_organ = load(child, large_path)
        stops = stop_map(large_organ)
        assert large_organ['stop_count'] == len(stops) == 2048
        assert len({stop['id'] for stop in stops.values()}) == 2048
        codes = {stop['midi_action_code'] for stop in stops.values()}
        assert len(codes) == 2048 and min(codes) > 255, (len(codes), min(codes))
        result['large'] = {'count': len(stops), 'min_action': min(codes), 'max_action': max(codes)}
        check('2048 complete registers load with distinct console IDs and wide MIDI action codes')
        play_edge(child, large_organ, 1); play_edge(child, large_organ, 2048)
        check('first and last of 2048 registers both trigger actual sampler voices')
        large_layout = layout_for_all(large_organ)
        assert sum(len(page['elements']) for page in large_layout['pages']) == 2048
        child.api('/settings/console_layout', {'organ_id': str(large_path), 'layout': large_layout})
        assert child.layout(large_path) == large_layout
        expected_bindings = [binding(stops[1]['midi_action_code'], 61), binding(stops[2048]['midi_action_code'], 60)]
        child.api('/test/preset_bindings', {'bindings': expected_bindings})
        trigger_action(child, 60, stops[2048]['midi_action_code'])
        trigger_action(child, 61, stops[1]['midi_action_code'])
        child.api('/settings/save', {})
        check('2048-control paginated console and wide MIDI learn bindings persist without truncation')
        child.stop(); child = None
        check('first owned process stopped before complete application restart')
        child = Child(exe, profile, work / 'headless-child-2.log')
        result['processes'].append({'pid': child.process.pid, 'port': child.port})
        assert result['processes'][0]['pid'] != child.process.pid
        organ = load(child, box_path)
        slots_after = box_slots(organ, boxes)
        assert {i: box['id'] for i, box in slots_after.items()} == {i: box['id'] for i, box in slots.items()}
        assert child.layout(box_path) == box_layout
        child.api('/settings/save', {})
        restored_bindings = child.api('/settings/organ')['swell_bindings']
        for index, enclosure in slots_after.items():
            saved = next(binding for binding in restored_bindings if binding['division_name'] == enclosure['swell_name'])
            assert saved['division_index'] == 128 + enclosure['slot'] and saved['cc_num'] == 11 + index
        check('rank enclosures, independent CC slots and console restore after process restart')
        large_organ = load(child, large_path)
        assert large_organ['stop_count'] == 2048 and child.layout(large_path) == large_layout
        restored_codes = stop_map(large_organ)
        trigger_action(child, 60, restored_codes[2048]['midi_action_code'])
        trigger_action(child, 61, restored_codes[1]['midi_action_code'])
        play_edge(child, large_organ, 2048)
        check('2048-register project, full console and high MIDI actions survive a fresh process')
        child.stop(); child = None
        after = {path: digest(Path(path)) for path in originals}
        assert after == originals
        result['source_hashes_after'] = after
        check('all source definitions, acoustic metadata and samples remain byte-identical')
        result['status'] = 'passed'
    except BaseException as error:
        result['status'], result['error'] = 'failed', str(error)
        raise
    finally:
        if child is not None:
            child.stop()
        result['count'] = len(result['checks'])
        (work / 'advanced-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        lock.unlink()
        print(json.dumps({'status': result['status'], 'count': result['count'], 'processes': result['processes'],
                          'error': result.get('error')}, indent=2), flush=True)


if __name__ == '__main__':
    main()
