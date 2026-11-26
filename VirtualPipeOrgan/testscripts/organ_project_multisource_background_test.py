"""Exercise three/four-source projects in owned windowless app processes.

Every synthetic HW source reuses stop/rank/pipe/package/wind IDs. Separate
package roots and different pitches make accidental source sharing observable.
Only processes started by this script are stopped; existing previews are ignored.
"""
import argparse
import copy
import json
import math
import os
from pathlib import Path
import sys
import time
import uuid
from xml.etree.ElementTree import Element, SubElement, tostring

from console_texture_background_test import Child, digest, norm, read_json
from organ_project_background_test import make_wave, write_fixture


PURPOSE = 'jm-orgue-project-multisource-background-test'
ALLOWED_DIR = 'orgelbouwer-meerbronnen-2026-11-26'
LETTERS = 'ABCDE'
REFERENCES = (415, 432, 460, 480, 500)
CENTS = (0, 100, 200, 300, 400)
SOURCE_IDS = [str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/' + letter)) for letter in LETTERS]
PROJECT_ID = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE))


def xml(index):
    document = Element('Hauptwerk', FileFormat='Organ', FileFormatVersion='4.0')

    def table(kind, rows):
        objects = SubElement(document, 'ObjectList', ObjectType=kind)
        for fields in rows:
            row = SubElement(objects, kind)
            for field, value in fields.items():
                SubElement(row, field).text = str(value)

    frequency = REFERENCES[index]
    table('_General', [{'Identification_Name': 'Meerbronnen ' + LETTERS[index],
                        'EnablePlayingAtOriginalOrganPitch': 'Y'}])
    table('Division', [{'DivisionID': 1, 'Name': 'Bronmanual'}])
    table('Stop', [{'StopID': 1, 'Name': 'Grondstem 8', 'DivisionID': 1}])
    table('StopRank', [{'StopID': 1, 'RankID': 1, 'MIDINoteNumOfFirstMappedDivisionInputNode': 69,
                       'NumberOfMappedDivisionInputNodes': 1}])
    table('Rank', [{'RankID': 1, 'Name': 'Grondstem 8'}])
    table('Pipe_SoundEngine01', [{'PipeID': 1, 'RankID': 1, 'NormalMIDINoteNumber': 69,
        'Pitch_Tempered_RankBasePitch64ftHarmonicNum': 8,
        'Pitch_OriginalOrgan_SpecificationMethodCode': 2, 'Pitch_OriginalOrgan_PitchHz': frequency,
        'WindSupply_SourceWindCompartmentID': 2,
        'WindSupply_MassFlowRateKilogramsPerSecAtReferencePressureDiff': .008,
        'WindSupply_ReferencePressureDifferenceInches': 4}])
    table('Pipe_SoundEngine01_Layer', [{'LayerID': 1, 'PipeID': 1, 'PipeLayerNumber': 1}])
    table('Pipe_SoundEngine01_AttackSample', [{'UniqueID': 1, 'LayerID': 1, 'SampleID': 1}])
    table('Sample', [{'SampleID': 1, 'InstallationPackageID': 1, 'SampleFilename': 'tone.wav',
                      'Pitch_SpecificationMethodCode': 4, 'Pitch_ExactSamplePitch': frequency}])
    table('WindCompartment', [
        {'WindCompartmentID': 1, 'Name': 'Ventilator', 'InfiniteVolume': 'Y', 'DefaultAirPressureInches': 4},
        {'WindCompartmentID': 2, 'Name': 'Windlade', 'InfiniteVolume': 'N',
         'StandardVolumeMetresCubed': .08, 'DefaultAirPressureInches': 4},
    ])
    table('WindCompartmentLinkage', [{'FirstWindCompartmentID': 1, 'SecondWindCompartmentID': 2,
        'MassFlowRateKilogramsPerSecAtReferencePressureDiff': .02,
        'ReferencePressureDifferenceInches': 1, 'ValveControlTypeCode': 1}])
    return tostring(document, encoding='utf-8', xml_declaration=True)


def prepare(work):
    sources = work / 'sources'
    sources.mkdir(exist_ok=True)
    paths, roots = [], []
    for index, letter in enumerate(LETTERS):
        root = sources / ('packages-' + letter)
        package = root / 'OrganInstallationPackages' / '000001'
        package.mkdir(parents=True, exist_ok=True)
        make_wave(package / 'tone.wav', 48000, REFERENCES[index], False)
        write_fixture(package / 'PackageID000001.InstallationPackageDefinition_Hauptwerk_xml', b'<Hauptwerk/>')
        path = sources / ('Bron ' + letter + '.Organ_Hauptwerk_xml')
        write_fixture(path, xml(index))
        paths.append(path)
        roots.append(root)
    hashes = {str(file): digest(file) for file in sources.rglob('*') if file.is_file()}
    return paths, roots, hashes


def stop_map(organ):
    return {stop['internal_stop_id']: stop for division in organ['divisions'] for stop in division['stops']}


def design_for(organ, stop_ids):
    appearances = ('drawknob', 'rocker-ivory', 'rocker-wood', 'tablet')
    stops = stop_map(organ)
    return {'version': 1, 'enabled': True, 'name': 'Eigen meerbronnenconsole', 'theme': 'walnut',
        'pages': [{'id': 'page-main', 'name': 'Alle bronnen', 'width': 1200, 'height': 640,
                   'elements': [{'id': 'register-' + str(sid), 'kind': 'stop',
                                 'target_id': stops[sid]['id'], 'label': '',
                                 'x': 40 + index * 180, 'y': 100, 'width': 130, 'height': 110,
                                 'appearance': appearances[index % len(appearances)]}
                                for index, sid in enumerate(stop_ids)]}]}


def main():
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', required=True, type=Path)
    parser.add_argument('--work-dir', required=True, type=Path)
    parser.add_argument('--prepare-only', action='store_true')
    args = parser.parse_args()
    assert args.exe.is_absolute() and args.work_dir.is_absolute()
    exe, work = args.exe.resolve(strict=True), args.work_dir.resolve()
    allowed = Path(__file__).resolve().parents[2] / 'testen' / ALLOWED_DIR
    work.relative_to(allowed.resolve())
    assert exe.name.lower() in ('vpo-app.exe', 'vpo-app')
    work.mkdir(parents=True, exist_ok=True)
    assert not work.is_symlink()
    write_fixture(work / '.jm-project-multisource-fixture', (PURPOSE + '\n').encode())
    lock = work / '.running'
    with lock.open('x', encoding='utf-8') as file:
        file.write(str(os.getpid()))
    result = {'status': 'running', 'checks': [], 'processes': [], 'snapshots': {}, 'audio': []}
    child = None

    def check(label):
        result['checks'].append(label)
        print('OK ' + label, flush=True)

    def wait_for(predicate, label, seconds=4):
        deadline = time.monotonic() + seconds
        latest = None
        while time.monotonic() < deadline:
            latest = child.api('/project/wind')
            if predicate(latest):
                result['snapshots'][label] = latest
                return latest
            time.sleep(.025)
        raise AssertionError(f'{label}: {latest}')

    def stop_notes():
        child.api('/notes/69/off', {})
        child.api('/panic', {})
        child.api('/stops/set', {'ids': []})
        time.sleep(.06)

    def load(path):
        child.api('/load_organ', {'path': str(path)})
        organ = child.api('/organ')
        assert norm(organ['id']) == norm(path)
        stop_notes()
        child.api('/settings/master?db=-35', {})
        child.api('/settings/wind?enabled=0', {})
        child.api('/settings/auto_retune?on=1', {})
        child.api('/reverb', {'mix': 0})
        return organ

    def apply_design(path, design):
        reply = child.api('/settings/console_layout', {'organ_id': str(path), 'layout': design})
        assert reply.get('warning') is None, reply
        assert child.layout(path) == design

    def measure(organ, sid, source_index, label):
        # Tuning is measured with both wind effects disabled; wind isolation is
        # tested separately, so intentional pressure-dependent pitch is excluded.
        stop_notes()
        child.api('/project/wind/enabled', {'enabled': False})
        child.api('/stops/set', {'ids': [stop_map(organ)[sid]['id']]})
        recordings = work / 'recordings'
        recordings.mkdir(exist_ok=True)
        path = recordings / (label + '.mp3')
        child.api('/record/start', {'path': str(path)})
        try:
            note = child.api('/notes/69/on?velocity=100', {})
            assert note['voices_triggered'] == 1, note
            time.sleep(1.25)
        finally:
            child.api('/notes/69/off', {})
            recording = child.api('/record/stop', {})
            child.api('/panic', {})
        expected = 440 * 2 ** (CENTS[source_index] / 1200)
        hz = recording['peak_hz']
        cents_error = 1200 * math.log2(hz / expected) if hz > 0 else None
        result['audio'].append({'label': label, 'stop_id': sid, 'source': LETTERS[source_index],
            'reference_hz': REFERENCES[source_index], 'source_cents': CENTS[source_index],
            'expected_hz': expected, 'peak_hz': hz, 'cents_error': cents_error,
            'rms': recording['rms'], 'frames_dropped': recording['frames_dropped'], 'recording': str(path)})
        assert recording['decoded_samples'] > 0 and recording['rms'] > .00001, recording
        assert recording['frames_dropped'] == 0, recording
        assert cents_error is not None and abs(cents_error) < 2, result['audio'][-1]

    def identity(path, expected_design):
        library = read_json(profile / 'organ_library.json')
        key = 'jm-project:' + PROJECT_ID
        assert key in library['settings'], library['settings'].keys()
        assert library['settings'][key]['console_layout'] == expected_design
        assert abs(library['settings'][key]['master_volume_db'] + 35) < .00001
        sidecar = work / ('.jm-settings-project-' + PROJECT_ID + '.json')
        portable = read_json(sidecar)
        assert portable['soort'] == 'project' and portable['naam'] == PROJECT_ID
        assert portable['instellingen']['console_layout'] == expected_design
        assert child.layout(path) == expected_design

    try:
        paths, roots, originals = prepare(work)
        result['source_hashes_before'] = originals
        if args.prepare_only:
            result['status'] = 'prepared'
            check('five isolated synthetic packages prepared without starting an app')
            return
        profile = work / 'profile'
        profile.mkdir(exist_ok=True)
        child = Child(exe, profile, work / 'child-1.log')
        result['processes'].append(child.process.pid)
        catalogs = [child.api('/project/source', {'path': str(path), 'package_root': str(root)})
                    for path, root in zip(paths, roots)]
        assert all(len(catalog['stops']) == 1 for catalog in catalogs)
        sources = [{'id': SOURCE_IDS[index], 'path': str(path), 'package_root': str(root),
                    'fingerprint': catalog['fingerprint'], 'resources': [], 'reference_a4_hz': REFERENCES[index],
                    'tuning_policy': 'normalize', 'tuning_cents': CENTS[index], 'perspective': None}
                   for index, (path, root, catalog) in enumerate(zip(paths, roots, catalogs))]
        stops = [{'id': 101 + index, 'source_id': SOURCE_IDS[index], 'source_stop_id': 1,
                  'division_id': 1, 'name': 'Bron ' + LETTERS[index]} for index in range(5)]
        project = {'schema_version': 1, 'id': PROJECT_ID, 'name': 'Vier onafhankelijke bronnen',
            'target_a4_hz': 440, 'next_stop_id': 104, 'next_division_id': 2,
            'sources': copy.deepcopy(sources[:3]), 'divisions': [{'id': 1, 'name': 'Gedeeld manual',
                'midi_channel': 2, 'first_midi_note': 69, 'key_count': 1, 'pedal': False}],
            'stops': copy.deepcopy(stops[:3]), 'couplers': []}
        project_path = work / 'Meer dan twee bronnen.jmorgan'
        child.api('/project/save', {'path': str(project_path), 'project': project})
        assert len(child.api('/project/read', {'path': str(project_path)})['sources']) == 3
        organ = load(project_path)
        assert organ['stop_count'] == 3
        design3 = design_for(organ, [101, 102, 103])
        apply_design(project_path, design3)
        child.api('/settings/save', {})
        identity(project_path, design3)
        check('three-source project saves and loads with UUID-keyed console settings')

        project['sources'].append(copy.deepcopy(sources[3]))
        project['stops'].append(copy.deepcopy(stops[3]))
        project['next_stop_id'] = 105
        child.api('/project/save', {'path': str(project_path), 'project': project})
        assert len(child.api('/project/read', {'path': str(project_path)})['sources']) == 4
        organ = load(project_path)
        assert organ['stop_count'] == 4 and child.layout(project_path) == design3
        design4 = design_for(organ, [101, 102, 103, 104])
        apply_design(project_path, design4)
        child.api('/settings/save', {})
        stable_ids = {sid: stop['id'] for sid, stop in stop_map(organ).items()}
        assert stable_ids == {sid: 'project_' + str(sid) for sid in (101, 102, 103, 104)}
        check('adding the fourth source preserves existing stop identities and the saved console')

        for index in range(4):
            measure(organ, 101 + index, index, 'bron-' + LETTERS[index])
        check('decoded recordings prove all four colliding-ID sources play with their separate pitch normalization and cents')

        child.api('/project/wind/enabled', {'enabled': True})
        initial_wind = wait_for(lambda value: len(value['nodes']) == 8 and len(value['pressure_ratios']) == 8, 'four-wind-networks')
        assert initial_wind['pipe_count'] == 4 and len(initial_wind['links']) == 4
        chests = [next(index for index, node in enumerate(initial_wind['nodes'])
                       if node['source_id'] == source_id and node['compartment_id'] == 2) for source_id in SOURCE_IDS[:4]]
        assert len(set(chests)) == 4
        for active in range(4):
            stop_notes()
            wait_for(lambda value: all(abs(value['pressure_ratios'][index] - 1) < .0001 for index in chests),
                     'rest-before-' + LETTERS[active])
            child.api('/stops/set', {'ids': [stable_ids[101 + active]]})
            note = child.api('/notes/69/on?velocity=100', {})
            assert note['voices_triggered'] == 1, note
            wait_for(lambda value: all(abs(value['pressure_ratios'][index] - (1 / 1.04 if source == active else 1)) < .0001
                                      for source, index in enumerate(chests)), 'only-source-' + LETTERS[active])
        stop_notes()
        child.api('/stops/set', {'ids': list(stable_ids.values())})
        note = child.api('/notes/69/on?velocity=100', {})
        assert note['voices_triggered'] == 4, note
        wait_for(lambda value: all(abs(value['pressure_ratios'][index] - 1 / 1.04) < .0001 for index in chests), 'all-four-sounding')
        stop_notes()
        check('each source consumes only its own wind graph and all four also sound simultaneously')

        project['sources'].reverse()
        project['stops'].reverse()
        child.api('/project/save', {'path': str(project_path), 'project': project})
        organ = load(project_path)
        assert {sid: stop['id'] for sid, stop in stop_map(organ).items()} == stable_ids
        assert child.layout(project_path) == design4
        identity(project_path, design4)
        measure(organ, 104, 3, 'bron-D-na-volgordewissel')
        check('source/register reordering preserves UUID settings, exact console targets and source-specific audio')

        # Removing B reserves its ID forever in this project. A newly added E
        # must use the next counter, never the removed console target 102.
        project['sources'] = [source for source in project['sources'] if source['id'] != SOURCE_IDS[1]]
        project['stops'] = [stop for stop in project['stops'] if stop['id'] != 102]
        child.api('/project/save', {'path': str(project_path), 'project': project})
        organ = load(project_path)
        assert set(stop_map(organ)) == {101, 103, 104}
        removed_hash = digest(project_path)
        reused = copy.deepcopy(project)
        reused['sources'].append(copy.deepcopy(sources[4]))
        reused_stop = copy.deepcopy(stops[4])
        reused_stop['id'] = 102
        reused['stops'].append(reused_stop)
        reused['next_stop_id'] = 106
        error = child.api('/project/save', {'path': str(project_path), 'project': reused}, expected=400)
        assert 'eerder gebruikt' in str(error).lower(), error
        assert digest(project_path) == removed_hash
        project['sources'].append(copy.deepcopy(sources[4]))
        project['stops'].append(copy.deepcopy(stops[4]))
        project['next_stop_id'] = 106
        child.api('/project/save', {'path': str(project_path), 'project': project})
        organ = load(project_path)
        assert set(stop_map(organ)) == {101, 103, 104, 105}
        child.api('/stops/set', {'ids': ['project_102']})
        assert child.api('/notes/69/on?velocity=100', {})['voices_triggered'] == 0
        stop_notes()
        measure(organ, 105, 4, 'nieuwe-bron-E')
        final_design = design_for(organ, [101, 103, 104, 105])
        apply_design(project_path, final_design)
        child.api('/settings/save', {})
        check('remove/add rejects reused stop IDs and the removed console target cannot trigger the new source')

        before_bad_organ = child.api('/organ')
        before_bad_layout = child.layout(project_path)
        before_bad_settings = child.api('/settings/organ')
        before_bad_hash = digest(project_path)
        before_bad_library_settings = read_json(profile / 'organ_library.json')['settings']
        too_many = copy.deepcopy(project)
        for index in range(61):
            source = copy.deepcopy(sources[0])
            source['id'] = str(uuid.uuid5(uuid.NAMESPACE_URL, PURPOSE + '/excess/' + str(index)))
            too_many['sources'].append(source)
        assert len(too_many['sources']) == 65
        error = child.api('/project/save', {'path': str(project_path), 'project': too_many}, expected=400)
        assert '64' in str(error) and 'bron' in str(error).lower(), error
        too_many_path = work / 'Afgewezen 65 bronnen.jmorgan'
        write_fixture(too_many_path, json.dumps(too_many, indent=2).encode())
        error = child.api('/load_organ', {'path': str(too_many_path)}, expected=500)
        assert '64' in str(error) and 'bron' in str(error).lower(), error
        assert child.api('/organ') == before_bad_organ
        assert child.layout(project_path) == before_bad_layout
        assert child.api('/settings/organ') == before_bad_settings
        assert read_json(profile / 'organ_library.json')['settings'] == before_bad_library_settings
        assert digest(project_path) == before_bad_hash
        check('65-source save/load is rejected at the shared 64-source limit without changing the current project or settings')

        # A copy under another filename retains the same project UUID, so its
        # settings are restored by identity rather than its current filename.
        relocated = work / 'Zelfde project andere naam.jmorgan'
        write_fixture(relocated, project_path.read_bytes())
        organ = load(relocated)
        assert set(stop_map(organ)) == {101, 103, 104, 105}
        identity(relocated, final_design)
        child.api('/settings/save', {})
        child.stop()
        child = None
        child = Child(exe, profile, work / 'child-2.log')
        result['processes'].append(child.process.pid)
        organ = load(relocated)
        assert {sid: stop['id'] for sid, stop in stop_map(organ).items()} == {
            sid: 'project_' + str(sid) for sid in (101, 103, 104, 105)}
        identity(relocated, final_design)
        for source_index, sid in ((0, 101), (2, 103), (3, 104), (4, 105)):
            measure(organ, sid, source_index, 'bron-' + LETTERS[source_index] + '-na-herstart')
        check('project UUID settings, console IDs and four-source tuning survive a renamed file and complete process restart')
        result['source_hashes_after'] = {str(file): digest(file) for file in paths[0].parent.rglob('*')
                                         if file.is_file() and not file.name.startswith('.jm-settings')}
        assert result['source_hashes_after'] == result['source_hashes_before']
        check('all five original source definitions, package markers and sample bytes remain unchanged')
        child.stop()
        child = None
        check('both owned headless processes stopped without touching existing app windows')
        result['status'] = 'passed'
    except BaseException as error:
        result['status'], result['error'] = 'failed', str(error)
        raise
    finally:
        if child is not None:
            child.stop()
        result['count'] = len(result['checks'])
        (work / 'multisource-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        lock.unlink()
        print(json.dumps({'status': result['status'], 'count': result['count'], 'processes': result['processes']}, indent=2), flush=True)


if __name__ == '__main__':
    main()
